#ifndef BACKEND_FFI_GATE_H
#define BACKEND_FFI_GATE_H

#define WIN32_LEAN_AND_MEAN
#include <windows.h>

typedef struct {
    SRWLOCK lock;
    CONDITION_VARIABLE idle_cv;
    LONG active_calls;
    int shutting_down;
    void *client;
} BackendFfiGate;

void backend_ffi_gate_init(BackendFfiGate *gate, void *client);
int backend_ffi_gate_try_acquire(BackendFfiGate *gate, void **client_out);
void backend_ffi_gate_release(BackendFfiGate *gate);
void backend_ffi_gate_begin_shutdown(BackendFfiGate *gate, void **client_out);

#endif
