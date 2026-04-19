#include <windows.h>
#include <process.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <wchar.h>

#include "qsoripper_ffi.h"

typedef int32_t (*fn_qsr_log_qso)(struct QsrClient *, const struct QsrLogQsoRequest *, struct QsrLogQsoResult *);
typedef int32_t (*fn_qsr_update_qso)(struct QsrClient *, const struct QsrUpdateQsoRequest *);
typedef int32_t (*fn_qsr_get_qso)(struct QsrClient *, const char *, struct QsrQsoDetail *);
typedef int32_t (*fn_qsr_delete_qso)(struct QsrClient *, const char *);
typedef int32_t (*fn_qsr_get_space_weather)(struct QsrClient *, struct QsrSpaceWeather *);

enum Field {
    FIELD_CALLSIGN, FIELD_BAND, FIELD_MODE,
    FIELD_RST_SENT, FIELD_RST_RCVD,
    FIELD_COMMENT, FIELD_NOTES,
    FIELD_FREQ, FIELD_DATE, FIELD_TIME,
    FIELD_TIME_OFF, FIELD_QTH, FIELD_WORKED_NAME,
    FIELD_TX_POWER, FIELD_SUBMODE, FIELD_CONTEST_ID,
    FIELD_SERIAL_SENT, FIELD_SERIAL_RCVD,
    FIELD_EXCHANGE_SENT, FIELD_EXCHANGE_RCVD,
    FIELD_PROP_MODE, FIELD_SAT_NAME, FIELD_SAT_MODE,
    FIELD_IOTA, FIELD_ARRL_SECTION, FIELD_WORKED_STATE, FIELD_WORKED_COUNTY,
    FIELD_SKCC,
    FIELD_COUNT
};

int qsr_test_ui_text_to_wide(const char *text, wchar_t *wbuf, int wbuf_len);
UINT qsr_test_ui_text_codepage(void);
void qsr_test_reset_state(void);
void qsr_test_set_backend_ffi(struct QsrClient *client, fn_qsr_log_qso log_qso_fn, fn_qsr_update_qso update_qso_fn);
void qsr_test_set_backend_ffi_get_delete_weather(struct QsrClient *client, fn_qsr_get_qso get_qso_fn, fn_qsr_delete_qso delete_qso_fn, fn_qsr_get_space_weather get_space_weather_fn);
void qsr_test_set_form_basics(const char *callsign, const char *date, const char *time_str);
void qsr_test_set_band_mode_indices(int band_idx, int mode_idx);
void qsr_test_set_freq_field(const char *freq);
void qsr_test_set_selected_recent_qso(const char *local_id, const char *callsign);
void qsr_test_set_focused_field(enum Field field);
void qsr_test_set_rig_enabled(int enabled);
void qsr_test_apply_rig_result(int connected, const char *freq_display, const char *freq_mhz, const char *band, const char *mode);
const char *qsr_test_get_freq_field(void);
void qsr_test_invoke_log_qso(void);
void qsr_test_invoke_load_selected_qso(void);
void qsr_test_invoke_delete_selected_qso(void);
void qsr_test_invoke_fetch_space_weather(void);

static HANDLE g_log_entered = NULL;
static HANDLE g_release_log = NULL;
static HANDLE g_load_entered = NULL;
static HANDLE g_release_load = NULL;
static HANDLE g_delete_entered = NULL;
static HANDLE g_release_delete = NULL;
static HANDLE g_weather_entered = NULL;
static HANDLE g_release_weather = NULL;

static int32_t __cdecl stub_log_qso(struct QsrClient *client, const struct QsrLogQsoRequest *req, struct QsrLogQsoResult *res)
{
    (void)client;
    (void)req;
    (void)res;
    SetEvent(g_log_entered);
    WaitForSingleObject(g_release_log, 3000);
    return 0;
}

static int32_t __cdecl stub_update_qso(struct QsrClient *client, const struct QsrUpdateQsoRequest *req)
{
    (void)client;
    (void)req;
    return 0;
}

static int32_t __cdecl stub_get_qso(struct QsrClient *client, const char *local_id, struct QsrQsoDetail *detail)
{
    (void)client;
    (void)local_id;
    (void)detail;
    SetEvent(g_load_entered);
    WaitForSingleObject(g_release_load, 3000);
    return 0;
}

static int32_t __cdecl stub_delete_qso(struct QsrClient *client, const char *local_id)
{
    (void)client;
    (void)local_id;
    SetEvent(g_delete_entered);
    WaitForSingleObject(g_release_delete, 3000);
    return 0;
}

static int32_t __cdecl stub_get_space_weather(struct QsrClient *client, struct QsrSpaceWeather *sw)
{
    (void)client;
    (void)sw;
    SetEvent(g_weather_entered);
    WaitForSingleObject(g_release_weather, 3000);
    return 0;
}

static unsigned __stdcall invoke_log_thread(void *arg)
{
    (void)arg;
    qsr_test_invoke_log_qso();
    return 0;
}

static unsigned __stdcall invoke_load_thread(void *arg)
{
    (void)arg;
    qsr_test_invoke_load_selected_qso();
    return 0;
}

static unsigned __stdcall invoke_delete_thread(void *arg)
{
    (void)arg;
    qsr_test_invoke_delete_selected_qso();
    return 0;
}

static unsigned __stdcall invoke_weather_thread(void *arg)
{
    (void)arg;
    qsr_test_invoke_fetch_space_weather();
    return 0;
}

static int fail(const char *msg)
{
    fprintf(stderr, "FAIL: %s\n", msg);
    return 1;
}

static int test_issue_262_utf8_conversion(void)
{
    if (qsr_test_ui_text_codepage() != CP_UTF8) {
        return fail("UI text conversion is not using CP_UTF8");
    }
    wchar_t wbuf[64];
    int wlen = qsr_test_ui_text_to_wide("caf\xC3\xA9", wbuf, 64);
    if (wlen <= 4) {
        return fail("UTF-8 conversion returned no content");
    }
    if (wbuf[0] != L'c' || wbuf[1] != L'a' || wbuf[2] != L'f' || wbuf[3] != 0x00E9) {
        return fail("UTF-8 text was not converted correctly in UI path");
    }
    return 0;
}

static int test_issue_199_rig_tuning_updates_freq_field(void)
{
    qsr_test_reset_state();
    qsr_test_set_rig_enabled(1);
    qsr_test_set_form_basics("K1ABC", "2026-01-02", "03:04");
    qsr_test_set_band_mode_indices(0, 0);
    qsr_test_set_focused_field(FIELD_CALLSIGN);
    qsr_test_set_freq_field("14.000.00");

    qsr_test_apply_rig_result(1, "14.225.00", "14.22500", "20M", "SSB");

    if (strcmp(qsr_test_get_freq_field(), "14.225.00") != 0) {
        return fail("Rig tuning did not refresh frequency field while callsign was populated");
    }
    return 0;
}

static int test_issue_263_log_is_non_blocking(void)
{
    qsr_test_reset_state();
    qsr_test_set_form_basics("K1ABC", "2026-01-02", "03:04");
    qsr_test_set_band_mode_indices(0, 0);
    qsr_test_set_backend_ffi((struct QsrClient *)0x1, stub_log_qso, stub_update_qso);

    g_log_entered = CreateEventW(NULL, TRUE, FALSE, NULL);
    g_release_log = CreateEventW(NULL, TRUE, FALSE, NULL);
    if (!g_log_entered || !g_release_log) {
        return fail("failed to create test events");
    }

    uintptr_t tid = _beginthreadex(NULL, 0, invoke_log_thread, NULL, 0, NULL);
    if (!tid) {
        return fail("failed to start log invocation thread");
    }
    HANDLE thread = (HANDLE)tid;

    if (WaitForSingleObject(g_log_entered, 3000) != WAIT_OBJECT_0) {
        return fail("log backend call was not invoked");
    }

    DWORD wait = WaitForSingleObject(thread, 100);
    SetEvent(g_release_log);
    WaitForSingleObject(thread, 3000);
    CloseHandle(thread);
    CloseHandle(g_log_entered);
    CloseHandle(g_release_log);

    if (wait != WAIT_OBJECT_0) {
        return fail("LogQso blocked caller while backend call was in flight");
    }

    return 0;
}

static int test_issue_263_load_selected_qso_is_non_blocking(void)
{
    qsr_test_reset_state();
    qsr_test_set_selected_recent_qso("load-1", "K1ABC");
    qsr_test_set_backend_ffi_get_delete_weather((struct QsrClient *)0x1, stub_get_qso, NULL, NULL);

    g_load_entered = CreateEventW(NULL, TRUE, FALSE, NULL);
    g_release_load = CreateEventW(NULL, TRUE, FALSE, NULL);
    if (!g_load_entered || !g_release_load) {
        return fail("failed to create load test events");
    }

    uintptr_t tid = _beginthreadex(NULL, 0, invoke_load_thread, NULL, 0, NULL);
    if (!tid) {
        return fail("failed to start load invocation thread");
    }
    HANDLE thread = (HANDLE)tid;

    if (WaitForSingleObject(g_load_entered, 3000) != WAIT_OBJECT_0) {
        return fail("load backend call was not invoked");
    }

    DWORD wait = WaitForSingleObject(thread, 100);
    SetEvent(g_release_load);
    WaitForSingleObject(thread, 3000);
    CloseHandle(thread);
    CloseHandle(g_load_entered);
    CloseHandle(g_release_load);

    if (wait != WAIT_OBJECT_0) {
        return fail("LoadSelectedQso blocked caller while backend call was in flight");
    }
    return 0;
}

static int test_issue_263_delete_selected_qso_is_non_blocking(void)
{
    qsr_test_reset_state();
    qsr_test_set_selected_recent_qso("delete-1", "K1ABC");
    qsr_test_set_backend_ffi_get_delete_weather((struct QsrClient *)0x1, NULL, stub_delete_qso, NULL);

    g_delete_entered = CreateEventW(NULL, TRUE, FALSE, NULL);
    g_release_delete = CreateEventW(NULL, TRUE, FALSE, NULL);
    if (!g_delete_entered || !g_release_delete) {
        return fail("failed to create delete test events");
    }

    uintptr_t tid = _beginthreadex(NULL, 0, invoke_delete_thread, NULL, 0, NULL);
    if (!tid) {
        return fail("failed to start delete invocation thread");
    }
    HANDLE thread = (HANDLE)tid;

    if (WaitForSingleObject(g_delete_entered, 3000) != WAIT_OBJECT_0) {
        return fail("delete backend call was not invoked");
    }

    DWORD wait = WaitForSingleObject(thread, 100);
    SetEvent(g_release_delete);
    WaitForSingleObject(thread, 3000);
    CloseHandle(thread);
    CloseHandle(g_delete_entered);
    CloseHandle(g_release_delete);

    if (wait != WAIT_OBJECT_0) {
        return fail("DeleteSelectedQso blocked caller while backend call was in flight");
    }
    return 0;
}

static int test_issue_263_fetch_space_weather_is_non_blocking(void)
{
    qsr_test_reset_state();
    qsr_test_set_backend_ffi_get_delete_weather((struct QsrClient *)0x1, NULL, NULL, stub_get_space_weather);

    g_weather_entered = CreateEventW(NULL, TRUE, FALSE, NULL);
    g_release_weather = CreateEventW(NULL, TRUE, FALSE, NULL);
    if (!g_weather_entered || !g_release_weather) {
        return fail("failed to create weather test events");
    }

    uintptr_t tid = _beginthreadex(NULL, 0, invoke_weather_thread, NULL, 0, NULL);
    if (!tid) {
        return fail("failed to start weather invocation thread");
    }
    HANDLE thread = (HANDLE)tid;

    if (WaitForSingleObject(g_weather_entered, 3000) != WAIT_OBJECT_0) {
        return fail("space weather backend call was not invoked");
    }

    DWORD wait = WaitForSingleObject(thread, 100);
    SetEvent(g_release_weather);
    WaitForSingleObject(thread, 3000);
    CloseHandle(thread);
    CloseHandle(g_weather_entered);
    CloseHandle(g_release_weather);

    if (wait != WAIT_OBJECT_0) {
        return fail("FetchSpaceWeather blocked caller while backend call was in flight");
    }
    return 0;
}

int main(void)
{
    int failures = 0;
    if (test_issue_262_utf8_conversion() != 0) failures++;
    if (test_issue_199_rig_tuning_updates_freq_field() != 0) failures++;
    if (test_issue_263_log_is_non_blocking() != 0) failures++;
    if (test_issue_263_load_selected_qso_is_non_blocking() != 0) failures++;
    if (test_issue_263_delete_selected_qso_is_non_blocking() != 0) failures++;
    if (test_issue_263_fetch_space_weather_is_non_blocking() != 0) failures++;
    if (failures != 0) {
        fprintf(stderr, "FAIL: %d regression test(s) failed\n", failures);
        return 1;
    }
    puts("PASS: win32 issue regressions");
    return 0;
}
