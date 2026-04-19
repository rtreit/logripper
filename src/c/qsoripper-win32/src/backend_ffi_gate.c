#include "backend_ffi_gate.h"

void backend_ffi_gate_init(BackendFfiGate *gate, void *client)
{
    if (!gate) return;
    InitializeSRWLock(&gate->lock);
    InitializeConditionVariable(&gate->idle_cv);
    gate->active_calls = 0;
    gate->shutting_down = 0;
    gate->client = client;
}

int backend_ffi_gate_try_acquire(BackendFfiGate *gate, void **client_out)
{
    if (!gate || !client_out) return 0;
    AcquireSRWLockExclusive(&gate->lock);
    if (gate->shutting_down || !gate->client) {
        ReleaseSRWLockExclusive(&gate->lock);
        return 0;
    }
    gate->active_calls++;
    *client_out = gate->client;
    ReleaseSRWLockExclusive(&gate->lock);
    return 1;
}

void backend_ffi_gate_release(BackendFfiGate *gate)
{
    if (!gate) return;
    AcquireSRWLockExclusive(&gate->lock);
    if (gate->active_calls > 0) {
        gate->active_calls--;
        if (gate->active_calls == 0 && gate->shutting_down) {
            WakeConditionVariable(&gate->idle_cv);
        }
    }
    ReleaseSRWLockExclusive(&gate->lock);
}

void backend_ffi_gate_begin_shutdown(BackendFfiGate *gate, void **client_out)
{
    if (client_out) *client_out = NULL;
    if (!gate) return;
    AcquireSRWLockExclusive(&gate->lock);
    gate->shutting_down = 1;
    while (gate->active_calls > 0) {
        SleepConditionVariableSRW(&gate->idle_cv, &gate->lock, INFINITE, 0);
    }
    if (client_out) {
        *client_out = gate->client;
    }
    gate->client = NULL;
    ReleaseSRWLockExclusive(&gate->lock);
}
