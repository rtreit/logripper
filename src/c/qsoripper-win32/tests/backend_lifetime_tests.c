#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <process.h>
#include <stdio.h>
#include <stdlib.h>

#include "backend_ffi_gate.h"

typedef struct {
    BackendFfiGate gate;
    HANDLE worker_entered;
    HANDLE release_worker;
    HANDLE shutdown_done;
    volatile LONG worker_acquired;
    volatile LONG shutdown_complete;
    void *shutdown_client;
} Harness;

static void fail(const char *msg)
{
    fprintf(stderr, "FAIL: %s\n", msg);
    exit(1);
}

static unsigned __stdcall worker_thread(void *param)
{
    Harness *h = (Harness *)param;
    void *client = NULL;
    if (!backend_ffi_gate_try_acquire(&h->gate, &client)) {
        fail("worker failed to acquire backend gate");
    }
    InterlockedExchange(&h->worker_acquired, 1);
    SetEvent(h->worker_entered);
    WaitForSingleObject(h->release_worker, INFINITE);
    backend_ffi_gate_release(&h->gate);
    return 0;
}

static unsigned __stdcall shutdown_thread(void *param)
{
    Harness *h = (Harness *)param;
    backend_ffi_gate_begin_shutdown(&h->gate, &h->shutdown_client);
    InterlockedExchange(&h->shutdown_complete, 1);
    SetEvent(h->shutdown_done);
    return 0;
}

int main(void)
{
    Harness h;
    ZeroMemory(&h, sizeof(h));
    h.worker_entered = CreateEventW(NULL, TRUE, FALSE, NULL);
    h.release_worker = CreateEventW(NULL, TRUE, FALSE, NULL);
    h.shutdown_done = CreateEventW(NULL, TRUE, FALSE, NULL);
    if (!h.worker_entered || !h.release_worker || !h.shutdown_done) {
        fail("failed to create events");
    }

    backend_ffi_gate_init(&h.gate, (void *)0x1);

    uintptr_t worker_thread_id = _beginthreadex(NULL, 0, worker_thread, &h, 0, NULL);
    if (!worker_thread_id) fail("failed to create worker thread");
    HANDLE worker_handle = (HANDLE)worker_thread_id;

    if (WaitForSingleObject(h.worker_entered, 3000) != WAIT_OBJECT_0) {
        fail("worker did not enter in time");
    }

    uintptr_t shutdown_thread_id = _beginthreadex(NULL, 0, shutdown_thread, &h, 0, NULL);
    if (!shutdown_thread_id) fail("failed to create shutdown thread");
    HANDLE shutdown_handle = (HANDLE)shutdown_thread_id;

    Sleep(100);
    if (WaitForSingleObject(h.shutdown_done, 0) == WAIT_OBJECT_0) {
        fail("shutdown completed before in-flight worker finished");
    }

    SetEvent(h.release_worker);

    if (WaitForSingleObject(h.shutdown_done, 3000) != WAIT_OBJECT_0) {
        fail("shutdown did not complete after worker release");
    }

    if (h.shutdown_client != (void *)0x1) {
        fail("shutdown did not return original client");
    }

    if (InterlockedCompareExchange(&h.shutdown_complete, 0, 0) != 1) {
        fail("shutdown completion flag was not set");
    }

    void *client_after_shutdown = NULL;
    if (backend_ffi_gate_try_acquire(&h.gate, &client_after_shutdown)) {
        fail("acquire unexpectedly succeeded after shutdown");
    }

    WaitForSingleObject(worker_handle, 3000);
    WaitForSingleObject(shutdown_handle, 3000);
    CloseHandle(worker_handle);
    CloseHandle(shutdown_handle);
    CloseHandle(h.worker_entered);
    CloseHandle(h.release_worker);
    CloseHandle(h.shutdown_done);

    puts("PASS: backend gate blocks shutdown until in-flight calls complete");
    return 0;
}
