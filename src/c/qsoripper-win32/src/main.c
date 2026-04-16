/* ==========================================================================
 * QsoRipper Win32 — Pure Win32/GDI ham radio logging application
 * Single-file C implementation using owner-drawn controls.
 * Compile: cl /W4 /WX /analyze /DUNICODE /D_UNICODE main.c /link user32.lib gdi32.lib
 *          shell32.lib comctl32.lib
 * ========================================================================== */

#define WIN32_LEAN_AND_MEAN
#define _CRT_SECURE_NO_WARNINGS
#include <windows.h>
#include <windowsx.h>
#include <commctrl.h>
#include <shellapi.h>
#include <process.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

/* ── Compile-time settings ─────────────────────────────────────────────── */

#define APP_TITLE       L"QsoRipper"
#define WINDOW_CLASS    L"QsoRipperWin32"
#define FONT_NAME       L"Consolas"
#define FONT_SIZE       14
#define TIMER_ID        1
#define TIMER_MS        100
#define STATUS_LIFETIME_MS  3000
#define LOOKUP_DEBOUNCE_MS  500
/* Tuning / behaviour constants */
#define MAX_FIELD_LEN   256

/* ── Menu item IDs ─────────────────────────────────────────────────────── */

#define IDM_FILE_EXIT        1001
#define IDM_HELP_KEYBOARD    1101
#define IDM_HELP_ABOUT       1102

/* Custom window messages */
#define WM_APP_LOOKUP_DONE   (WM_APP + 1)
#define WM_APP_RIG_DONE      (WM_APP + 2)
#define WM_APP_QSO_LOADED    (WM_APP + 3)

/* ── Color palette (Win2K classic theme) ────────────────────────────────── */

#define CLR_BG          RGB(212, 208, 200)  /* COLOR_BTNFACE */
#define CLR_FIELD_BG    RGB(255, 255, 255)  /* White field background */
#define CLR_TEXT        RGB(0, 0, 0)        /* Black text */
#define CLR_CYAN        RGB(0, 128, 128)    /* Teal for accents */
#define CLR_YELLOW      RGB(255, 255, 0)
#define CLR_ORANGE      RGB(220, 100, 0)
#define CLR_WHITE       RGB(255, 255, 255)
#define CLR_GRAY        RGB(50, 50, 50)
#define CLR_DARKGRAY    RGB(20, 20, 20)
#define CLR_GREEN           RGB(0, 128, 0)
#define CLR_BRIGHT_GREEN    RGB(0, 255, 0)
#define CLR_RED         RGB(192, 0, 0)
#define CLR_BLUE_BG     RGB(0, 0, 128)      /* Navy selection */
#define CLR_FORM_BORDER RGB(128, 128, 128)  /* Group box border */
#define CLR_MAGENTA     RGB(128, 0, 128)
#define CLR_FOOTER_BG   RGB(0, 0, 128)      /* Navy footer */
#define CLR_FOOTER_FG   RGB(255, 255, 255)  /* White footer text */
#define CLR_LABEL       RGB(0, 0, 0)        /* Black labels */
#define CLR_HEADER_BG   RGB(0, 0, 128)      /* Navy header */
#define CLR_HEADER_FG   RGB(255, 255, 255)  /* White header text */
#define CLR_HIGHLIGHT   RGB(0, 0, 128)      /* Selection highlight */
#define CLR_HILITE_FG   RGB(255, 255, 255)  /* Selection text */

/* ── Band / Mode constants ─────────────────────────────────────────────── */

static const char *BANDS[] = {
    "160M","80M","60M","40M","30M","20M","17M","15M","12M","10M","6M","2M","70CM"
};
static const char *MODES[] = {
    "SSB","CW","FT8","FT4","RTTY","PSK31","AM","FM"
};
static const double BAND_DEFAULT_FREQS[] = {
    1.900, 3.750, 5.330, 7.150, 10.125, 14.225, 18.100,
    21.200, 24.940, 28.400, 50.125, 146.520, 446.000
};
#define DEFAULT_BAND_IDX 5
#define NUM_BANDS 13
#define NUM_MODES 8

/* ── Form fields ───────────────────────────────────────────────────────── */

enum Field {
    FIELD_CALLSIGN, FIELD_BAND, FIELD_MODE,
    FIELD_RST_SENT, FIELD_RST_RCVD,
    FIELD_COMMENT,  FIELD_NOTES,
    FIELD_FREQ,     FIELD_DATE, FIELD_TIME,
    /* Advanced fields */
    FIELD_TIME_OFF, FIELD_QTH, FIELD_WORKED_NAME,
    FIELD_TX_POWER, FIELD_SUBMODE, FIELD_CONTEST_ID,
    FIELD_SERIAL_SENT, FIELD_SERIAL_RCVD,
    FIELD_EXCHANGE_SENT, FIELD_EXCHANGE_RCVD,
    FIELD_PROP_MODE, FIELD_SAT_NAME, FIELD_SAT_MODE,
    FIELD_IOTA, FIELD_ARRL_SECTION, FIELD_WORKED_STATE, FIELD_WORKED_COUNTY,
    FIELD_SKCC,
    FIELD_COUNT
};

static const char *FIELD_LABELS[] = {
    "Callsign", "Band", "Mode",
    "RST Sent", "RST Rcvd",
    "Comment",  "Notes",
    "Freq MHz", "Date", "Time",
    /* Advanced labels */
    "Time Off", "QTH", "Name",
    "TX Power", "Submode", "Contest ID",
    "Serial Sent", "Serial Rcvd",
    "Exch Sent", "Exch Rcvd",
    "Prop Mode", "Sat Name", "Sat Mode",
    "IOTA", "ARRL Sect", "State", "County", "SKCC"
};

static const int FIELD_MAX_LEN[] = {
    30, 0, 0,   /* callsign, band(cycle), mode(cycle) */
    6,  6,      /* rst sent, rst rcvd */
    250, 250,   /* comment, notes */
    14, 14, 14, /* freq, date, time */
    /* Advanced max lengths */
    14, 60, 60,         /* time_off, qth, worked_name */
    14, 14, 30,         /* tx_power, submode, contest_id */
    14, 14,             /* serial_sent, serial_rcvd */
    60, 60,             /* exchange_sent, exchange_rcvd */
    14, 30, 14,         /* prop_mode, sat_name, sat_mode */
    14, 14, 14, 30, 16  /* iota, arrl_section, worked_state, worked_county, skcc */
};

/* ── Async lookup message structs ──────────────────────────────────────── */

typedef struct {
    HWND hwnd;
    char callsign[64];
} LookupThreadArg;

typedef struct {
    char callsign[64];
    char name[64];
    char qth[64];
    char grid[16];
    char country[64];
    int  cq_zone;
    int  has_data;
} LookupResultMsg;

/* ── Rig poll message structs ──────────────────────────────────────────── */

typedef struct {
    HWND hwnd;
} RigPollArg;

typedef struct {
    int  connected;
    char freq_display[32];
    char freq_mhz[16];
    char band[8];
    char mode[16];
} RigPollResult;

/* ── Recent QSO record ─────────────────────────────────────────────────── */

typedef struct {
    char utc[24];
    char callsign[16];
    char band[8];
    char mode[8];
    char rst_sent[8];
    char rst_rcvd[8];
    char country[32];
    char grid[8];
    char local_id[64];
} RecentQso;

/* ── Application state ─────────────────────────────────────────────────── */

typedef struct {
    /* Focus / navigation */
    enum Field focused_field;
    int band_idx, mode_idx;
    int qso_list_focused;
    int search_focused;
    int field_all_selected;   /* 1 = next keypress replaces entire field */
    int qso_selected;        /* -1 = none */
    int running;
    int help_visible;

    /* QSO timer */
    int qso_timer_active;
    ULONGLONG qso_started_at;

    /* Form field buffers */
    char callsign[32];
    char rst_sent[8];
    char rst_rcvd[8];
    char comment[256];
    char notes[256];
    char freq_mhz[16];
    char date[16];
    char time_str[16];

    /* Advanced view state */
    int advanced_view;        /* 0 = basic, 1 = advanced */
    int advanced_tab;         /* 0=Main, 1=Contest, 2=Technical, 3=Awards */

    /* Advanced field buffers */
    char time_off[16];
    char qth[64];
    char worked_name[64];
    char tx_power[16];
    char submode[16];
    char contest_id[32];
    char serial_sent[16];
    char serial_rcvd[16];
    char exchange_sent[64];
    char exchange_rcvd[64];
    char prop_mode[16];
    char sat_name[32];
    char sat_mode[16];
    char iota[16];
    char arrl_section[16];
    char worked_state[16];
    char worked_county[32];
    char skcc[16];

    /* Cursor positions per field */
    int cursor_pos[FIELD_COUNT];

    /* Hit-test rectangles for click-to-focus (populated during paint) */
    RECT field_rects[FIELD_COUNT];
    int  field_rects_valid;
    int  qso_list_y;  /* top of QSO list panel for click detection */
    int  qso_list_row_h; /* row height in QSO list */

    /* Search */
    char search_text[64];
    int search_cursor;

    /* Status bar */
    char status_text[256];
    int status_is_error;
    ULONGLONG status_created_at;

    /* Lookup */
    char lookup_name[64];
    char lookup_qth[64];
    char lookup_grid[16];
    char lookup_country[64];
    int  lookup_cq_zone;
    int  has_lookup;
    int  lookup_in_progress;

    /* Recent QSOs (heap-allocated; grows as needed) */
    RecentQso *recent_qsos;
    int recent_count;
    int recent_capacity;
    int qso_loading;

    /* Scroll offset for QSO list */
    int qso_scroll;
    int qso_page_size;

    /* Space weather */
    double k_index;
    double solar_flux;
    int sunspot_number;
    int has_weather;

    /* Rig control */
    int  rig_enabled;
    int  rig_connected;
    int  rig_poll_in_progress;
    ULONGLONG last_rig_poll;
    char rig_freq_display[32];
    char rig_freq_mhz[16];
    char rig_band[8];
    char rig_mode[16];

    /* Editing existing QSO */
    char editing_local_id[64];

    /* Lookup debounce */
    ULONGLONG last_callsign_change;
    char last_looked_up[32];

    /* Confirm delete dialog */
    int confirm_delete_visible;

    /* GDI objects */
    HFONT hFont;
    HFONT hFontBold;
    HFONT hFontSmall;
    HFONT hFontSmallBold;
    int char_w, char_h;
    int list_cw, list_ch;

    HWND hwnd;
} AppState;

static AppState g_state;

/* ── Forward declarations ──────────────────────────────────────────────── */

static LRESULT CALLBACK WndProc(HWND, UINT, WPARAM, LPARAM);
static void PaintAll(HWND hwnd, HDC hdc, RECT *rc);
static void OnKeyDown(HWND hwnd, WPARAM vk, LPARAM lp);
static void OnChar(HWND hwnd, WPARAM ch);
static void OnTimer(HWND hwnd);
static void InitState(void);
static void ClearForm(void);
static void SetStatus(const char *msg, int is_error);
static void LogQso(void);
static void RefreshQsoList(void);
static void RefreshQsoListAsync(HWND hwnd);
static void FetchSpaceWeather(void);
static void LoadSelectedQso(void);
static void DeleteSelectedQso(void);
static int  PaintAdvancedForm(HDC hdc, int y_start, int w);

static char *RunQrCommand(const char *args);
static char *FieldBuffer(enum Field f);
static int   FieldMaxLen(enum Field f);
static void  DrawField(HDC, int, int, int, const char *, int, int, int, int);
static void  DrawCycleField(HDC, int, int, int, const char *, int, int, int);
static void  ApplyModeDefaults(void);

/* ── Utility: safe string helpers ──────────────────────────────────────── */

static void safe_strcpy(char *dst, size_t dstsz, const char *src)
{
    if (!src) { dst[0] = 0; return; }
    size_t len = strlen(src);
    if (len >= dstsz) len = dstsz - 1;
    memcpy(dst, src, len);
    dst[len] = 0;
}

static void safe_strcat(char *dst, size_t dstsz, const char *src)
{
    size_t cur = strlen(dst);
    size_t avail = dstsz - cur;
    if (avail <= 1) return;
    safe_strcpy(dst + cur, avail, src);
}

/* ── Minimal JSON value extractor (no dependency) ──────────────────────── */

/* Finds "key": "value" or "key": number in a JSON string.
   Returns a malloc'd string with the value, or NULL.  */
static char *json_get_string(const char *json, const char *key)
{
    char pattern[128];
    snprintf(pattern, sizeof(pattern), "\"%s\"", key);
    const char *p = strstr(json, pattern);
    if (!p) return NULL;
    p += strlen(pattern);
    while (*p == ' ' || *p == ':' || *p == '\t') p++;
    if (*p == '"') {
        p++;
        const char *end = strchr(p, '"');
        if (!end) return NULL;
        size_t len = (size_t)(end - p);
        char *val = (char *)malloc(len + 1);
        if (!val) return NULL;
        memcpy(val, p, len);
        val[len] = 0;
        return val;
    }
    /* Numeric or boolean */
    {
        const char *end = p;
        while (*end && *end != ',' && *end != '}' && *end != ' ' && *end != '\n') end++;
        size_t len = (size_t)(end - p);
        char *val = (char *)malloc(len + 1);
        if (!val) return NULL;
        memcpy(val, p, len);
        val[len] = 0;
        return val;
    }
}

static double json_get_double(const char *json, const char *key, double dflt)
{
    char *v = json_get_string(json, key);
    if (!v) return dflt;
    double d = atof(v);
    free(v);
    return d;
}

static int json_get_int(const char *json, const char *key, int dflt)
{
    char *v = json_get_string(json, key);
    if (!v) return dflt;
    int i = atoi(v);
    free(v);
    return i;
}

/* ── Find entries in a JSON array ──────────────────────────────────────── */

/* Returns pointer to the nth { in a JSON array, or NULL */
static const char *json_array_nth(const char *json, int n)
{
    const char *p = strchr(json, '[');
    if (!p) return NULL;
    p++;
    int depth = 0, idx = 0;
    for (; *p; p++) {
        if (*p == '{') {
            if (depth == 0) {
                if (idx == n) return p;
                idx++;
            }
            depth++;
        } else if (*p == '}') {
            depth--;
        } else if (*p == ']' && depth == 0) {
            break;
        }
    }
    return NULL;
}

/* Extract a substring from { to matching } inclusive */
static char *json_extract_object(const char *start)
{
    if (!start || *start != '{') return NULL;
    int depth = 0;
    const char *p = start;
    for (; *p; p++) {
        if (*p == '{') depth++;
        else if (*p == '}') { depth--; if (depth == 0) { p++; break; } }
    }
    size_t len = (size_t)(p - start);
    char *obj = (char *)malloc(len + 1);
    if (!obj) return NULL;
    memcpy(obj, start, len);
    obj[len] = 0;
    return obj;
}

/* ── CLI runner: execute `qr <args>` and capture stdout ────────────────── */

static char g_cli_path[MAX_PATH];

static void FindCliPath(void)
{
    /* Try to find QsoRipper.Cli.exe relative to our own exe:
       We're at: .../artifacts/publish/qsoripper-win32/Release/qsoripper-win32.exe
       CLI is at: .../artifacts/publish/QsoRipper.Cli/Release/QsoRipper.Cli.exe */
    char module[MAX_PATH];
    GetModuleFileNameA(NULL, module, MAX_PATH);

    /* Walk up to artifacts/publish */
    char *p = strrchr(module, '\\');
    if (p) *p = 0; /* strip exe name */
    p = strrchr(module, '\\');
    if (p) *p = 0; /* strip Release */
    p = strrchr(module, '\\');
    if (p) *p = 0; /* strip qsoripper-win32 */

    /* Try sibling directory */
    snprintf(g_cli_path, MAX_PATH, "%s\\QsoRipper.Cli\\Release\\QsoRipper.Cli.exe", module);
    if (GetFileAttributesA(g_cli_path) != INVALID_FILE_ATTRIBUTES)
        return;

    /* Try QSORIPPER_CLI_PATH env var */
    if (GetEnvironmentVariableA("QSORIPPER_CLI_PATH", g_cli_path, MAX_PATH) > 0)
        if (GetFileAttributesA(g_cli_path) != INVALID_FILE_ATTRIBUTES)
            return;

    /* Fallback: assume on PATH */
    safe_strcpy(g_cli_path, MAX_PATH, "QsoRipper.Cli.exe");
}

static char *RunQrCommand(const char *args)
{
    SECURITY_ATTRIBUTES sa = {0};
    sa.nLength = sizeof(sa);
    sa.bInheritHandle = TRUE;

    HANDLE hReadPipe, hWritePipe;
    if (!CreatePipe(&hReadPipe, &hWritePipe, &sa, 0))
        return NULL;
    SetHandleInformation(hReadPipe, HANDLE_FLAG_INHERIT, 0);

    char cmdline[8192];
    snprintf(cmdline, sizeof(cmdline), "\"%s\" %s", g_cli_path, args);

    STARTUPINFOA si = {0};
    si.cb = sizeof(si);
    si.dwFlags = STARTF_USESTDHANDLES | STARTF_USESHOWWINDOW;
    si.hStdOutput = hWritePipe;
    si.hStdError  = hWritePipe;
    si.hStdInput  = INVALID_HANDLE_VALUE;
    si.wShowWindow = SW_HIDE;

    PROCESS_INFORMATION pi = {0};

    if (!CreateProcessA(NULL, cmdline, NULL, NULL, TRUE,
                        CREATE_NO_WINDOW, NULL, NULL, &si, &pi)) {
        CloseHandle(hReadPipe);
        CloseHandle(hWritePipe);
        return NULL;
    }
    CloseHandle(hWritePipe);

    /* Read all stdout */
    char *output = (char *)malloc(8192);
    if (!output) {
        WaitForSingleObject(pi.hProcess, 5000);
        CloseHandle(pi.hProcess);
        CloseHandle(pi.hThread);
        CloseHandle(hReadPipe);
        return NULL;
    }
    DWORD totalRead = 0;
    DWORD capacity = 8192;
    for (;;) {
        if (totalRead + 1024 > capacity) {
            DWORD newCapacity = capacity * 2;
            char *grown = (char *)realloc(output, newCapacity);
            if (!grown) {
                free(output);
                WaitForSingleObject(pi.hProcess, 5000);
                CloseHandle(pi.hProcess);
                CloseHandle(pi.hThread);
                CloseHandle(hReadPipe);
                return NULL;
            }
            output = grown;
            capacity = newCapacity;
        }
        DWORD bytesRead = 0;
        if (!ReadFile(hReadPipe, output + totalRead, 1024, &bytesRead, NULL) || bytesRead == 0)
            break;
        totalRead += bytesRead;
    }
    output[totalRead] = 0;

    WaitForSingleObject(pi.hProcess, 5000);
    CloseHandle(pi.hProcess);
    CloseHandle(pi.hThread);
    CloseHandle(hReadPipe);

    return output;
}

/* ── Field buffer accessor ─────────────────────────────────────────────── */

static char *FieldBuffer(enum Field f)
{
    switch (f) {
    case FIELD_CALLSIGN:      return g_state.callsign;
    case FIELD_RST_SENT:      return g_state.rst_sent;
    case FIELD_RST_RCVD:      return g_state.rst_rcvd;
    case FIELD_COMMENT:       return g_state.comment;
    case FIELD_NOTES:         return g_state.notes;
    case FIELD_FREQ:          return g_state.freq_mhz;
    case FIELD_DATE:          return g_state.date;
    case FIELD_TIME:          return g_state.time_str;
    case FIELD_TIME_OFF:      return g_state.time_off;
    case FIELD_QTH:           return g_state.qth;
    case FIELD_WORKED_NAME:   return g_state.worked_name;
    case FIELD_TX_POWER:      return g_state.tx_power;
    case FIELD_SUBMODE:       return g_state.submode;
    case FIELD_CONTEST_ID:    return g_state.contest_id;
    case FIELD_SERIAL_SENT:   return g_state.serial_sent;
    case FIELD_SERIAL_RCVD:   return g_state.serial_rcvd;
    case FIELD_EXCHANGE_SENT: return g_state.exchange_sent;
    case FIELD_EXCHANGE_RCVD: return g_state.exchange_rcvd;
    case FIELD_PROP_MODE:     return g_state.prop_mode;
    case FIELD_SAT_NAME:      return g_state.sat_name;
    case FIELD_SAT_MODE:      return g_state.sat_mode;
    case FIELD_IOTA:          return g_state.iota;
    case FIELD_ARRL_SECTION:  return g_state.arrl_section;
    case FIELD_WORKED_STATE:  return g_state.worked_state;
    case FIELD_WORKED_COUNTY: return g_state.worked_county;
    case FIELD_SKCC:          return g_state.skcc;
    default: return NULL; /* band/mode are cycle selectors, FIELD_COUNT sentinel */
    }
}

static int FieldMaxLen(enum Field f)
{
    if (f >= 0 && f < FIELD_COUNT)
        return FIELD_MAX_LEN[f];
    return 0;
}

/* ── GDI drawing helpers ───────────────────────────────────────────────── */

static void DrawText_A(HDC hdc, int x, int y, COLORREF fg, const char *text)
{
    SetTextColor(hdc, fg);
    SetBkMode(hdc, TRANSPARENT);
    int len = (int)strlen(text);
    /* Convert to wide */
    wchar_t wbuf[1024];
    int wlen = MultiByteToWideChar(CP_ACP, 0, text, len, wbuf, 1024);
    TextOutW(hdc, x, y, wbuf, wlen);
}

static void DrawText_A_BG(HDC hdc, int x, int y, COLORREF fg, COLORREF bg, const char *text)
{
    SetTextColor(hdc, fg);
    SetBkColor(hdc, bg);
    SetBkMode(hdc, OPAQUE);
    int len = (int)strlen(text);
    wchar_t wbuf[1024];
    int wlen = MultiByteToWideChar(CP_ACP, 0, text, len, wbuf, 1024);
    TextOutW(hdc, x, y, wbuf, wlen);
    SetBkMode(hdc, TRANSPARENT);
}

static void FillRect_Color(HDC hdc, int x, int y, int w, int h, COLORREF clr)
{
    RECT r = { x, y, x + w, y + h };
    HBRUSH br = CreateSolidBrush(clr);
    FillRect(hdc, &r, br);
    DeleteObject(br);
}

static void DrawBox(HDC hdc, int x, int y, int w, int h, COLORREF border)
{
    HPEN pen = CreatePen(PS_SOLID, 1, border);
    HPEN oldPen = (HPEN)SelectObject(hdc, pen);
    HBRUSH oldBr = (HBRUSH)SelectObject(hdc, GetStockObject(NULL_BRUSH));
    Rectangle(hdc, x, y, x + w, y + h);
    SelectObject(hdc, oldBr);
    SelectObject(hdc, oldPen);
    DeleteObject(pen);
}

static void DrawHLine(HDC hdc, int x1, int x2, int y, COLORREF clr)
{
    HPEN pen = CreatePen(PS_SOLID, 1, clr);
    HPEN old = (HPEN)SelectObject(hdc, pen);
    MoveToEx(hdc, x1, y, NULL);
    LineTo(hdc, x2, y);
    SelectObject(hdc, old);
    DeleteObject(pen);
}

static char FieldHotkey(enum Field f)
{
    switch (f) {
    case FIELD_CALLSIGN:      return 'C';
    case FIELD_BAND:          return 'B';
    case FIELD_MODE:          return 'M';
    case FIELD_RST_SENT:      return 'S';
    case FIELD_RST_RCVD:      return 'R';
    case FIELD_COMMENT:       return 'O';
    case FIELD_NOTES:         return 'N';
    case FIELD_FREQ:          return 'Z';
    case FIELD_DATE:          return 'D';
    case FIELD_TIME:          return 'T';
    case FIELD_WORKED_NAME:   return 'A';
    case FIELD_TIME_OFF:      return 'I';
    case FIELD_QTH:           return 'Q';
    case FIELD_TX_POWER:      return 'W';
    case FIELD_SUBMODE:       return 'U';
    case FIELD_SERIAL_SENT:   return 'E';
    case FIELD_PROP_MODE:     return 'P';
    case FIELD_SKCC:          return 'K';
    default:                  return 0;
    }
}

static void DrawLabelWithHotkey(HDC hdc, int x, int y, COLORREF fg,
                                const char *label, char hotkey, int cw, int ch)
{
    DrawText_A(hdc, x, y, fg, label);
    if (!hotkey) return;
    hotkey = (char)toupper((unsigned char)hotkey);
    int len = (int)strlen(label);
    for (int i = 0; i < len; i++) {
        if (toupper((unsigned char)label[i]) == hotkey) {
            DrawHLine(hdc, x + i * cw, x + (i + 1) * cw,
                      y + ch, fg);
            return;
        }
    }
}

/* Draw a chip-style label: colored background with text */
static void DrawChip(HDC hdc, int x, int y, COLORREF bg, COLORREF fg,
                     const char *text, int cw, int ch)
{
    int tw = (int)strlen(text) * cw + 8;
    FillRect_Color(hdc, x, y, tw, ch + 2, bg);
    DrawText_A(hdc, x + 4, y + 1, fg, text);
}

/* ── Draw a text field (owner-drawn edit box) ──────────────────────────── */

static void DrawField(HDC hdc, int x, int y, int width_chars,
                      const char *value, int cursor, int focused,
                      int cw, int ch)
{
    int box_w = width_chars * cw + 6;
    int box_h = ch + 4;

    /* Background — white field with sunken-style border */
    COLORREF bg = CLR_FIELD_BG;
    COLORREF fg = focused ? CLR_TEXT : CLR_DARKGRAY;
    FillRect_Color(hdc, x, y, box_w, box_h, bg);

    /* 3D sunken border */
    if (focused) {
        DrawBox(hdc, x, y, box_w, box_h, CLR_HIGHLIGHT);
    } else {
        HPEN dark = CreatePen(PS_SOLID, 1, RGB(128, 128, 128));
        HPEN light = CreatePen(PS_SOLID, 1, RGB(255, 255, 255));
        HPEN old = (HPEN)SelectObject(hdc, dark);
        MoveToEx(hdc, x, y + box_h - 1, NULL);
        LineTo(hdc, x, y); LineTo(hdc, x + box_w - 1, y);
        SelectObject(hdc, light);
        MoveToEx(hdc, x + box_w - 1, y, NULL);
        LineTo(hdc, x + box_w - 1, y + box_h - 1); LineTo(hdc, x, y + box_h - 1);
        SelectObject(hdc, old);
        DeleteObject(dark); DeleteObject(light);
    }

    /* Text and cursor */
    if (focused && g_state.field_all_selected && value && value[0]) {
        int text_w = (int)strlen(value) * cw;
        FillRect_Color(hdc, x + 3, y + 2, text_w, ch, CLR_HIGHLIGHT);
        DrawText_A_BG(hdc, x + 3, y + 2, CLR_HILITE_FG, CLR_HIGHLIGHT, value);
    } else {
        if (value)
            DrawText_A_BG(hdc, x + 3, y + 2, fg, bg, value);

        /* Cursor */
        if (focused) {
            int cx = x + 3 + cursor * cw;
            HPEN pen = CreatePen(PS_SOLID, 2, CLR_TEXT);
            HPEN old = (HPEN)SelectObject(hdc, pen);
            MoveToEx(hdc, cx, y + 2, NULL);
            LineTo(hdc, cx, y + 2 + ch);
            SelectObject(hdc, old);
            DeleteObject(pen);
        }
    }
}

/* Draw a cycle selector (Band/Mode) with ◀ ▶ indicators */
static void DrawCycleField(HDC hdc, int x, int y, int width_chars,
                           const char *value, int focused, int cw, int ch)
{
    int box_w = width_chars * cw + 6;
    int box_h = ch + 4;

    COLORREF bg = CLR_FIELD_BG;
    FillRect_Color(hdc, x, y, box_w, box_h, bg);
    /* Sunken border, highlight when focused */
    if (focused) {
        DrawBox(hdc, x, y, box_w, box_h, CLR_HIGHLIGHT);
    } else {
        HPEN dark = CreatePen(PS_SOLID, 1, RGB(128, 128, 128));
        HPEN light = CreatePen(PS_SOLID, 1, RGB(255, 255, 255));
        HPEN old = (HPEN)SelectObject(hdc, dark);
        MoveToEx(hdc, x, y + box_h - 1, NULL);
        LineTo(hdc, x, y); LineTo(hdc, x + box_w - 1, y);
        SelectObject(hdc, light);
        MoveToEx(hdc, x + box_w - 1, y, NULL);
        LineTo(hdc, x + box_w - 1, y + box_h - 1); LineTo(hdc, x, y + box_h - 1);
        SelectObject(hdc, old);
        DeleteObject(dark); DeleteObject(light);
    }

    COLORREF fg = focused ? CLR_HIGHLIGHT : CLR_DARKGRAY;

    /* Arrow indicators */
    if (focused) {
        char buf[64];
        snprintf(buf, sizeof(buf), "< %s >", value);
        DrawText_A_BG(hdc, x + 3, y + 2, fg, bg, buf);
    } else {
        DrawText_A_BG(hdc, x + 3, y + 2, fg, bg, value);
    }
}

/* ── Populate date/time with current UTC ───────────────────────────────── */

static void SetCurrentDateTime(void)
{
    SYSTEMTIME st;
    GetSystemTime(&st);
    snprintf(g_state.date, sizeof(g_state.date),
              "%04d-%02d-%02d", st.wYear, st.wMonth, st.wDay);
    snprintf(g_state.time_str, sizeof(g_state.time_str),
              "%02d:%02d", st.wHour, st.wMinute);
}

/* ── Set focused field with select-all ──────────────────────────────────── */

static void SetFocusField(enum Field f)
{
    g_state.focused_field = f;
    g_state.field_all_selected = (f != FIELD_BAND && f != FIELD_MODE);
}

/* ── Initialize application state ──────────────────────────────────────── */

static void InitState(void)
{
    memset(&g_state, 0, sizeof(g_state));
    g_state.focused_field = FIELD_CALLSIGN;
    g_state.field_all_selected = 1;
    g_state.band_idx = DEFAULT_BAND_IDX;
    g_state.mode_idx = 0;
    g_state.qso_selected = -1;
    g_state.running = 1;
    safe_strcpy(g_state.rst_sent, sizeof(g_state.rst_sent), "59");
    safe_strcpy(g_state.rst_rcvd, sizeof(g_state.rst_rcvd), "59");

    snprintf(g_state.freq_mhz, sizeof(g_state.freq_mhz),
              "%.3f", BAND_DEFAULT_FREQS[DEFAULT_BAND_IDX]);

    SetCurrentDateTime();
}

static void ClearForm(void)
{
    g_state.callsign[0] = 0;
    g_state.comment[0] = 0;
    g_state.notes[0] = 0;
    snprintf(g_state.freq_mhz, sizeof(g_state.freq_mhz),
              "%.3f", BAND_DEFAULT_FREQS[g_state.band_idx]);
    SetCurrentDateTime();

    g_state.has_lookup = 0;
    g_state.editing_local_id[0] = 0;
    g_state.qso_timer_active = 0;
    g_state.qso_started_at = 0;
    g_state.last_looked_up[0] = 0;
    SetFocusField(FIELD_CALLSIGN);
    g_state.qso_list_focused = 0;
    g_state.search_focused = 0;
    memset(g_state.cursor_pos, 0, sizeof(g_state.cursor_pos));
    ApplyModeDefaults();

    /* Clear advanced field buffers */
    g_state.time_off[0] = 0;
    g_state.qth[0] = 0;
    g_state.worked_name[0] = 0;
    g_state.tx_power[0] = 0;
    g_state.submode[0] = 0;
    g_state.contest_id[0] = 0;
    g_state.serial_sent[0] = 0;
    g_state.serial_rcvd[0] = 0;
    g_state.exchange_sent[0] = 0;
    g_state.exchange_rcvd[0] = 0;
    g_state.prop_mode[0] = 0;
    g_state.sat_name[0] = 0;
    g_state.sat_mode[0] = 0;
    g_state.iota[0] = 0;
    g_state.arrl_section[0] = 0;
    g_state.worked_state[0] = 0;
    g_state.worked_county[0] = 0;
    g_state.skcc[0] = 0;
}

static void ApplyModeDefaults(void)
{
    const char *mode = MODES[g_state.mode_idx];
    const char *rst = (_stricmp(mode, "CW") == 0 || _stricmp(mode, "RTTY") == 0)
                      ? "599" : "59";
    safe_strcpy(g_state.rst_sent, sizeof(g_state.rst_sent), rst);
    safe_strcpy(g_state.rst_rcvd, sizeof(g_state.rst_rcvd), rst);
    g_state.cursor_pos[FIELD_RST_SENT] = (int)strlen(rst);
    g_state.cursor_pos[FIELD_RST_RCVD] = (int)strlen(rst);
}


static void SetStatus(const char *msg, int is_error)
{
    safe_strcpy(g_state.status_text, sizeof(g_state.status_text), msg);
    g_state.status_is_error = is_error;
    g_state.status_created_at = GetTickCount64();
}

/* ── CLI integration: Log QSO ──────────────────────────────────────────── */

static void AppendArg(char *cmd, size_t cmd_sz, const char *flag, const char *val)
{
    if (!val || !val[0]) return;
    safe_strcat(cmd, cmd_sz, " ");
    safe_strcat(cmd, cmd_sz, flag);
    safe_strcat(cmd, cmd_sz, " \"");
    safe_strcat(cmd, cmd_sz, val);
    safe_strcat(cmd, cmd_sz, "\"");
}

static void LogQso(void)
{
    if (g_state.callsign[0] == 0) {
        SetStatus("Callsign is required", 1);
        return;
    }

    char cmd[4096];
    int is_update = g_state.editing_local_id[0] != 0;

    if (is_update) {
        snprintf(cmd, sizeof(cmd),
                  "update %s --band %s --mode %s --at \"%s %s\" "
                  "--rst-sent %s --rst-rcvd %s",
                  g_state.editing_local_id,
                  BANDS[g_state.band_idx],
                  MODES[g_state.mode_idx],
                  g_state.date, g_state.time_str,
                  g_state.rst_sent,
                  g_state.rst_rcvd);
    } else {
        snprintf(cmd, sizeof(cmd),
                  "log %s %s %s --at \"%s %s\" --rst-sent %s --rst-rcvd %s",
                  g_state.callsign,
                  BANDS[g_state.band_idx],
                  MODES[g_state.mode_idx],
                  g_state.date, g_state.time_str,
                  g_state.rst_sent,
                  g_state.rst_rcvd);
    }

    if (g_state.freq_mhz[0]) {
        unsigned long long freq_khz =
            (unsigned long long)(atof(g_state.freq_mhz) * 1000.0 + 0.5);
        if (freq_khz > 0) {
            char freq_arg[32];
            snprintf(freq_arg, sizeof(freq_arg), " --freq %llu", freq_khz);
            safe_strcat(cmd, sizeof(cmd), freq_arg);
        }
    }

    AppendArg(cmd, sizeof(cmd), "--comment", g_state.comment);
    AppendArg(cmd, sizeof(cmd), "--notes", g_state.notes);
    AppendArg(cmd, sizeof(cmd), "--name", g_state.worked_name);
    AppendArg(cmd, sizeof(cmd), "--tx-power", g_state.tx_power);
    AppendArg(cmd, sizeof(cmd), "--submode", g_state.submode);
    AppendArg(cmd, sizeof(cmd), "--contest-id", g_state.contest_id);
    AppendArg(cmd, sizeof(cmd), "--serial-sent", g_state.serial_sent);
    AppendArg(cmd, sizeof(cmd), "--serial-rcvd", g_state.serial_rcvd);
    AppendArg(cmd, sizeof(cmd), "--exchange-sent", g_state.exchange_sent);
    AppendArg(cmd, sizeof(cmd), "--exchange-rcvd", g_state.exchange_rcvd);
    AppendArg(cmd, sizeof(cmd), "--prop-mode", g_state.prop_mode);
    AppendArg(cmd, sizeof(cmd), "--sat-name", g_state.sat_name);
    AppendArg(cmd, sizeof(cmd), "--sat-mode", g_state.sat_mode);
    AppendArg(cmd, sizeof(cmd), "--iota", g_state.iota);
    AppendArg(cmd, sizeof(cmd), "--arrl-section", g_state.arrl_section);
    AppendArg(cmd, sizeof(cmd), "--state", g_state.worked_state);
    AppendArg(cmd, sizeof(cmd), "--worked-county", g_state.worked_county);
    AppendArg(cmd, sizeof(cmd), "--skcc", g_state.skcc);

    if (g_state.time_off[0] && g_state.date[0]) {
        char time_off_str[64];
        snprintf(time_off_str, sizeof(time_off_str), "%s %s",
                  g_state.date, g_state.time_off);
        AppendArg(cmd, sizeof(cmd), "--time-off", time_off_str);
    }

    char *result = RunQrCommand(cmd);
    if (result) {
        if (is_update) {
            SetStatus("QSO updated", 0);
        } else {
            char msg[128];
            snprintf(msg, sizeof(msg), "Logged %s on %s %s",
                      g_state.callsign, BANDS[g_state.band_idx],
                      MODES[g_state.mode_idx]);
            SetStatus(msg, 0);
        }
        free(result);
    } else {
        SetStatus("Failed to run qr command", 1);
    }

    ClearForm();
    RefreshQsoList();
}

/* ── CLI integration: Refresh QSO list ─────────────────────────────────── */

typedef struct {
    HWND hwnd;
} QsoLoadArg;

typedef struct {
    RecentQso *qsos;
    int count;
    int capacity;
} QsoLoadResult;

static unsigned __stdcall QsoLoadThread(void *param)
{
    QsoLoadArg *arg = (QsoLoadArg *)param;
    QsoLoadResult *res = (QsoLoadResult *)calloc(1, sizeof(QsoLoadResult));
    if (!res) { free(arg); return 0; }

    char *result = RunQrCommand("list --limit 0 --json");
    if (result) {
        for (int i = 0; ; i++) {
            const char *obj_start = json_array_nth(result, i);
            if (!obj_start) break;

            char *obj = json_extract_object(obj_start);
            if (!obj) break;

            if (res->count >= res->capacity) {
                int new_cap = res->capacity == 0 ? 64 : res->capacity * 2;
                RecentQso *tmp = (RecentQso *)realloc(res->qsos,
                                                       (size_t)new_cap * sizeof(RecentQso));
                if (!tmp) { free(obj); break; }
                res->qsos = tmp;
                res->capacity = new_cap;
            }

            RecentQso *q = &res->qsos[res->count];
            memset(q, 0, sizeof(*q));

            char *v;
            v = json_get_string(obj, "utcTimestamp");
            if (!v) v = json_get_string(obj, "utc");
            if (v) {
                if (strlen(v) >= 16) {
                    char buf[6];
                    buf[0] = v[11]; buf[1] = v[12]; buf[2] = ':';
                    buf[3] = v[14]; buf[4] = v[15]; buf[5] = 0;
                    safe_strcpy(q->utc, sizeof(q->utc), buf);
                } else {
                    safe_strcpy(q->utc, sizeof(q->utc), v);
                }
                free(v);
            }

            v = json_get_string(obj, "workedCallsign");
            if (!v) v = json_get_string(obj, "callsign");
            if (v) { safe_strcpy(q->callsign, sizeof(q->callsign), v); free(v); }

            v = json_get_string(obj, "band");
            if (v) {
                const char *display = v;
                if (strncmp(display, "BAND_", 5) == 0) display += 5;
                safe_strcpy(q->band, sizeof(q->band), display);
                free(v);
            }

            v = json_get_string(obj, "mode");
            if (v) {
                const char *display = v;
                if (strncmp(display, "MODE_", 5) == 0) display += 5;
                safe_strcpy(q->mode, sizeof(q->mode), display);
                free(v);
            }

            {
                int r = json_get_int(obj, "readability", 0);
                int s = json_get_int(obj, "strength", 0);
                int t = json_get_int(obj, "tone", 0);
                if (r > 0) {
                    if (t > 0) snprintf(q->rst_sent, sizeof(q->rst_sent), "%d%d%d", r, s, t);
                    else       snprintf(q->rst_sent, sizeof(q->rst_sent), "%d%d", r, s);
                }
            }

            {
                const char *rcvd_pos = strstr(obj, "rstReceived");
                if (rcvd_pos) {
                    int r = json_get_int(rcvd_pos, "readability", 0);
                    int s = json_get_int(rcvd_pos, "strength", 0);
                    int t = json_get_int(rcvd_pos, "tone", 0);
                    if (r > 0) {
                        if (t > 0) snprintf(q->rst_rcvd, sizeof(q->rst_rcvd), "%d%d%d", r, s, t);
                        else       snprintf(q->rst_rcvd, sizeof(q->rst_rcvd), "%d%d", r, s);
                    }
                }
            }

            v = json_get_string(obj, "country");
            if (!v) {
                const char *snap = strstr(obj, "stationSnapshot");
                if (snap) v = json_get_string(snap, "country");
            }
            if (v) { safe_strcpy(q->country, sizeof(q->country), v); free(v); }

            v = json_get_string(obj, "workedGrid");
            if (!v) v = json_get_string(obj, "grid");
            if (v) { safe_strcpy(q->grid, sizeof(q->grid), v); free(v); }

            v = json_get_string(obj, "localId");
            if (!v) v = json_get_string(obj, "local_id");
            if (v) { safe_strcpy(q->local_id, sizeof(q->local_id), v); free(v); }

            res->count++;
            free(obj);
        }
        free(result);
    }

    if (!PostMessage(arg->hwnd, WM_APP_QSO_LOADED, 0, (LPARAM)res))
        free(res);

    free(arg);
    return 0;
}

static void RefreshQsoListAsync(HWND hwnd)
{
    QsoLoadArg *arg = (QsoLoadArg *)malloc(sizeof(QsoLoadArg));
    if (!arg) return;
    arg->hwnd = hwnd;
    g_state.qso_loading = 1;
    HANDLE h = (HANDLE)_beginthreadex(NULL, 0, QsoLoadThread, arg, 0, NULL);
    if (h) CloseHandle(h);
    else { free(arg); g_state.qso_loading = 0; }
}

static void RefreshQsoList(void)
{
    RefreshQsoListAsync(g_state.hwnd);
}

/* ── CLI integration: Lookup callsign ──────────────────────────────────── */

static void ClearLookupDisplay(void)
{
    g_state.has_lookup = 0;
    g_state.lookup_in_progress = 0;
    g_state.lookup_name[0] = 0;
    g_state.lookup_qth[0] = 0;
    g_state.lookup_grid[0] = 0;
    g_state.lookup_country[0] = 0;
    g_state.lookup_cq_zone = 0;
    g_state.last_looked_up[0] = 0;
}

static unsigned __stdcall LookupThread(void *param)
{
    LookupThreadArg *arg = (LookupThreadArg *)param;
    LookupResultMsg *res = (LookupResultMsg *)calloc(1, sizeof(LookupResultMsg));
    if (!res) { free(arg); return 0; }

    safe_strcpy(res->callsign, sizeof(res->callsign), arg->callsign);

    char cmd[128];
    snprintf(cmd, sizeof(cmd), "lookup %s --json", arg->callsign);
    char *result = RunQrCommand(cmd);

    if (result) {
        const char *record_pos = strstr(result, "\"record\"");
        if (record_pos) {
            char *v;
            v = json_get_string(record_pos, "formattedName");
            if (!v) v = json_get_string(record_pos, "firstName");
            if (v) {
                safe_strcpy(res->name, sizeof(res->name), v);
                res->has_data = 1;
                free(v);
            }
            v = json_get_string(record_pos, "addr2");
            if (!v) v = json_get_string(record_pos, "qth");
            if (v) { safe_strcpy(res->qth, sizeof(res->qth), v); free(v); }

            v = json_get_string(record_pos, "gridSquare");
            if (!v) v = json_get_string(record_pos, "grid");
            if (v) { safe_strcpy(res->grid, sizeof(res->grid), v); free(v); }

            v = json_get_string(record_pos, "country");
            if (v) { safe_strcpy(res->country, sizeof(res->country), v); free(v); }

            res->cq_zone = json_get_int(record_pos, "cqZone", 0);
        }
        free(result);
    }

    if (!PostMessage(arg->hwnd, WM_APP_LOOKUP_DONE, 0, (LPARAM)res))
        free(res);

    free(arg);
    return 0;
}

/* ── CLI integration: Rig snapshot poll ────────────────────────────────── */

static unsigned __stdcall RigPollThread(void *param)
{
    RigPollArg *arg = (RigPollArg *)param;
    RigPollResult *res = (RigPollResult *)calloc(1, sizeof(RigPollResult));
    if (!res) { free(arg); return 0; }

    char *result = RunQrCommand("rig-status --json");
    if (result) {
        char *status = json_get_string(result, "status");
        if (status && _stricmp(status, "connected") == 0) {
            char *v;
            res->connected = 1;
            v = json_get_string(result, "frequencyDisplay");
            if (v) { safe_strcpy(res->freq_display, sizeof(res->freq_display), v); free(v); }
            v = json_get_string(result, "frequencyMhz");
            if (v) { safe_strcpy(res->freq_mhz, sizeof(res->freq_mhz), v); free(v); }
            v = json_get_string(result, "band");
            if (v) { safe_strcpy(res->band, sizeof(res->band), v); free(v); }
            v = json_get_string(result, "mode");
            if (v) { safe_strcpy(res->mode, sizeof(res->mode), v); free(v); }
        }
        if (status) free(status);
        free(result);
    }

    if (!PostMessage(arg->hwnd, WM_APP_RIG_DONE, 0, (LPARAM)res))
        free(res);

    free(arg);
    return 0;
}

/* ── CLI integration: Fetch space weather ──────────────────────────────── */

static void FetchSpaceWeather(void)
{
    char *result = RunQrCommand("space-weather --json");
    if (!result) return;

    g_state.k_index = json_get_double(result, "planetaryKIndex", 0.0);
    if (g_state.k_index == 0.0)
        g_state.k_index = json_get_double(result, "k_index", 0.0);
    g_state.solar_flux = json_get_double(result, "solarFluxIndex", 0.0);
    if (g_state.solar_flux == 0.0)
        g_state.solar_flux = json_get_double(result, "solar_flux", 0.0);
    g_state.sunspot_number = json_get_int(result, "sunspotNumber", 0);
    if (g_state.sunspot_number == 0)
        g_state.sunspot_number = json_get_int(result, "sunspot_number", 0);
    g_state.has_weather = 1;

    free(result);
}

/* ── CLI integration: Load selected QSO into form ──────────────────────── */

static void LoadSelectedQso(void)
{
    if (g_state.qso_selected < 0 || g_state.qso_selected >= g_state.recent_count)
        return;

    RecentQso *q = &g_state.recent_qsos[g_state.qso_selected];
    if (q->local_id[0] == 0) {
        SetStatus("No QSO ID for selection", 1);
        return;
    }

    char cmd[128];
    snprintf(cmd, sizeof(cmd), "get %s --json", q->local_id);
    char *result = RunQrCommand(cmd);
    if (!result) {
        SetStatus("Failed to load QSO data", 1);
        return;
    }

    ClearForm();

    char *v;

    v = json_get_string(result, "workedCallsign");
    if (!v) v = json_get_string(result, "callsign");
    if (v) { safe_strcpy(g_state.callsign, sizeof(g_state.callsign), v); free(v); }

    v = json_get_string(result, "band");
    if (v) {
        const char *disp = v;
        if (strncmp(disp, "BAND_", 5) == 0) disp += 5;
        for (int i = 0; i < NUM_BANDS; i++) {
            if (_stricmp(BANDS[i], disp) == 0) { g_state.band_idx = i; break; }
        }
        free(v);
    }

    v = json_get_string(result, "mode");
    if (v) {
        const char *disp = v;
        if (strncmp(disp, "MODE_", 5) == 0) disp += 5;
        for (int i = 0; i < NUM_MODES; i++) {
            if (_stricmp(MODES[i], disp) == 0) { g_state.mode_idx = i; break; }
        }
        free(v);
    }

    v = json_get_string(result, "frequencyKhz");
    if (v) {
        double khz = atof(v);
        if (khz > 0)
            snprintf(g_state.freq_mhz, sizeof(g_state.freq_mhz), "%.3f", khz / 1000.0);
        free(v);
    } else {
        snprintf(g_state.freq_mhz, sizeof(g_state.freq_mhz),
                  "%.3f", BAND_DEFAULT_FREQS[g_state.band_idx]);
    }

    v = json_get_string(result, "utcTimestamp");
    if (v && strlen(v) >= 16) {
        char date_buf[12];
        memcpy(date_buf, v, 10); date_buf[10] = 0;
        safe_strcpy(g_state.date, sizeof(g_state.date), date_buf);
        char time_buf[8];
        memcpy(time_buf, v + 11, 5); time_buf[5] = 0;
        safe_strcpy(g_state.time_str, sizeof(g_state.time_str), time_buf);
    }
    if (v) free(v);

    v = json_get_string(result, "utcEndTimestamp");
    if (v && strlen(v) >= 16) {
        char time_buf[8];
        memcpy(time_buf, v + 11, 5); time_buf[5] = 0;
        safe_strcpy(g_state.time_off, sizeof(g_state.time_off), time_buf);
    }
    if (v) free(v);

    {
        int r = json_get_int(result, "readability", 0);
        int s = json_get_int(result, "strength", 0);
        int t = json_get_int(result, "tone", 0);
        if (r > 0) {
            if (t > 0) snprintf(g_state.rst_sent, sizeof(g_state.rst_sent), "%d%d%d", r, s, t);
            else       snprintf(g_state.rst_sent, sizeof(g_state.rst_sent), "%d%d", r, s);
        } else {
            safe_strcpy(g_state.rst_sent, sizeof(g_state.rst_sent), "59");
        }
    }

    {
        const char *rcvd_pos = strstr(result, "rstReceived");
        if (rcvd_pos) {
            int r = json_get_int(rcvd_pos, "readability", 0);
            int s = json_get_int(rcvd_pos, "strength", 0);
            int t = json_get_int(rcvd_pos, "tone", 0);
            if (r > 0) {
                if (t > 0) snprintf(g_state.rst_rcvd, sizeof(g_state.rst_rcvd), "%d%d%d", r, s, t);
                else       snprintf(g_state.rst_rcvd, sizeof(g_state.rst_rcvd), "%d%d", r, s);
            } else {
                safe_strcpy(g_state.rst_rcvd, sizeof(g_state.rst_rcvd), "59");
            }
        } else {
            safe_strcpy(g_state.rst_rcvd, sizeof(g_state.rst_rcvd), "59");
        }
    }

    v = json_get_string(result, "comment");
    if (v) { safe_strcpy(g_state.comment, sizeof(g_state.comment), v); free(v); }

    v = json_get_string(result, "notes");
    if (v) { safe_strcpy(g_state.notes, sizeof(g_state.notes), v); free(v); }

    v = json_get_string(result, "workedOperatorName");
    if (v) { safe_strcpy(g_state.worked_name, sizeof(g_state.worked_name), v); free(v); }

    v = json_get_string(result, "txPower");
    if (v) { safe_strcpy(g_state.tx_power, sizeof(g_state.tx_power), v); free(v); }

    v = json_get_string(result, "submode");
    if (v) { safe_strcpy(g_state.submode, sizeof(g_state.submode), v); free(v); }

    v = json_get_string(result, "contestId");
    if (v) { safe_strcpy(g_state.contest_id, sizeof(g_state.contest_id), v); free(v); }

    v = json_get_string(result, "serialSent");
    if (v) { safe_strcpy(g_state.serial_sent, sizeof(g_state.serial_sent), v); free(v); }

    v = json_get_string(result, "serialReceived");
    if (v) { safe_strcpy(g_state.serial_rcvd, sizeof(g_state.serial_rcvd), v); free(v); }

    v = json_get_string(result, "exchangeSent");
    if (v) { safe_strcpy(g_state.exchange_sent, sizeof(g_state.exchange_sent), v); free(v); }

    v = json_get_string(result, "exchangeReceived");
    if (v) { safe_strcpy(g_state.exchange_rcvd, sizeof(g_state.exchange_rcvd), v); free(v); }

    v = json_get_string(result, "propMode");
    if (v) { safe_strcpy(g_state.prop_mode, sizeof(g_state.prop_mode), v); free(v); }

    v = json_get_string(result, "satName");
    if (v) { safe_strcpy(g_state.sat_name, sizeof(g_state.sat_name), v); free(v); }

    v = json_get_string(result, "satMode");
    if (v) { safe_strcpy(g_state.sat_mode, sizeof(g_state.sat_mode), v); free(v); }

    v = json_get_string(result, "workedIota");
    if (v) { safe_strcpy(g_state.iota, sizeof(g_state.iota), v); free(v); }

    v = json_get_string(result, "workedArrlSection");
    if (v) { safe_strcpy(g_state.arrl_section, sizeof(g_state.arrl_section), v); free(v); }

    v = json_get_string(result, "workedState");
    if (v) { safe_strcpy(g_state.worked_state, sizeof(g_state.worked_state), v); free(v); }

    v = json_get_string(result, "workedCounty");
    if (v) { safe_strcpy(g_state.worked_county, sizeof(g_state.worked_county), v); free(v); }

    v = json_get_string(result, "skcc");
    if (v) { safe_strcpy(g_state.skcc, sizeof(g_state.skcc), v); free(v); }

    safe_strcpy(g_state.editing_local_id, sizeof(g_state.editing_local_id), q->local_id);

    g_state.cursor_pos[FIELD_CALLSIGN] = (int)strlen(g_state.callsign);
    g_state.cursor_pos[FIELD_RST_SENT] = (int)strlen(g_state.rst_sent);
    g_state.cursor_pos[FIELD_RST_RCVD] = (int)strlen(g_state.rst_rcvd);
    g_state.cursor_pos[FIELD_FREQ]     = (int)strlen(g_state.freq_mhz);
    g_state.cursor_pos[FIELD_DATE]     = (int)strlen(g_state.date);
    g_state.cursor_pos[FIELD_TIME]     = (int)strlen(g_state.time_str);

    g_state.qso_list_focused = 0;
    SetFocusField(FIELD_CALLSIGN);
    free(result);

    SetStatus("QSO loaded for editing", 0);
}

/* ── CLI integration: Delete selected QSO ──────────────────────────────── */

static void DeleteSelectedQso(void)
{
    if (g_state.qso_selected < 0 || g_state.qso_selected >= g_state.recent_count)
        return;

    RecentQso *q = &g_state.recent_qsos[g_state.qso_selected];
    if (q->local_id[0] == 0) {
        SetStatus("No QSO ID available for delete", 1);
        return;
    }

    char cmd[128];
    snprintf(cmd, sizeof(cmd), "delete %s", q->local_id);
    char *result = RunQrCommand(cmd);
    if (result) {
        char msg[128];
        snprintf(msg, sizeof(msg), "Deleted QSO with %s", q->callsign);
        SetStatus(msg, 0);
        free(result);
    } else {
        SetStatus("Failed to delete QSO", 1);
    }

    g_state.qso_selected = -1;
    g_state.confirm_delete_visible = 0;
    RefreshQsoList();
}

/* ── Drawing: Header bar ───────────────────────────────────────────────── */

static void PaintHeader(HDC hdc, RECT *rc)
{
    int cw = g_state.char_w;
    int ch = g_state.char_h;
    int header_h = ch * 3 + 4;
    int w = rc->right - rc->left;

    FillRect_Color(hdc, 0, 0, w, header_h, CLR_HEADER_BG);

    /* Left: title */
    SelectObject(hdc, g_state.hFontBold);
    DrawText_A(hdc, cw, ch, CLR_HEADER_FG, "QsoRipper");
    SelectObject(hdc, g_state.hFont);

    /* Center: space weather */
    if (g_state.has_weather) {
        char weather[128];
        COLORREF kclr = g_state.k_index <= 3 ? CLR_BRIGHT_GREEN :
                        g_state.k_index <= 5 ? CLR_YELLOW : CLR_RED;
        snprintf(weather, sizeof(weather),
                  "K:%.0f  SFI:%.0f  SN:%d",
                  g_state.k_index, g_state.solar_flux,
                  g_state.sunspot_number);
        int tw = (int)strlen(weather) * cw;
        int cx = (w - tw) / 2;
        /* Draw K-index colored, rest white */
        char kbuf[16];
        snprintf(kbuf, sizeof(kbuf), "K:%.0f", g_state.k_index);
        DrawText_A_BG(hdc, cx, ch, kclr, CLR_HEADER_BG, kbuf);
        int kw = (int)strlen(kbuf) * cw;
        char rest[100];
        snprintf(rest, sizeof(rest), "  SFI:%.0f  SN:%d",
                  g_state.solar_flux, g_state.sunspot_number);
        DrawText_A_BG(hdc, cx + kw, ch, CLR_HEADER_FG, CLR_HEADER_BG, rest);
    } else {
        int cx = (w - 22 * cw) / 2;
        DrawText_A_BG(hdc, cx, ch, CLR_HEADER_FG, CLR_HEADER_BG, "Loading space weather...");
    }

    /* Right: UTC clock */
    {
        SYSTEMTIME st;
        GetSystemTime(&st);
        char clk[32];
        snprintf(clk, sizeof(clk), "%02d:%02d:%02d UTC",
                  st.wHour, st.wMinute, st.wSecond);
        int tw = (int)strlen(clk) * cw;
        DrawText_A_BG(hdc, w - tw - cw, ch, CLR_YELLOW, CLR_HEADER_BG, clk);
    }

    /* Row 2: rig status */
    {
        int ry = ch * 2;
        if (!g_state.rig_enabled) {
            DrawText_A_BG(hdc, cw, ry, CLR_HEADER_FG, CLR_HEADER_BG, "  Rig: OFF");
        } else if (g_state.rig_connected && g_state.rig_freq_display[0]) {
            char rigbuf[64];
            snprintf(rigbuf, sizeof(rigbuf), "* %s  %s",
                      g_state.rig_freq_display, g_state.rig_mode);
            DrawText_A_BG(hdc, cw, ry, CLR_BRIGHT_GREEN, CLR_HEADER_BG, rigbuf);
        } else {
            DrawText_A_BG(hdc, cw, ry, CLR_YELLOW, CLR_HEADER_BG, "? Rig: waiting...");
        }
    }
}

/* ── Drawing: Status bar ───────────────────────────────────────────────── */

static int PaintStatus(HDC hdc, int y, int w)
{
    int ch = g_state.char_h;
    int bar_h = ch + 4;

    FillRect_Color(hdc, 0, y, w, bar_h, CLR_BG);

    if (g_state.status_text[0] != 0) {
        ULONGLONG elapsed = GetTickCount64() - g_state.status_created_at;
        if (elapsed < STATUS_LIFETIME_MS) {
            COLORREF clr = g_state.status_is_error ? CLR_RED : CLR_GREEN;
            DrawText_A(hdc, g_state.char_w * 2, y + 2, clr, g_state.status_text);
        } else {
            g_state.status_text[0] = 0;
        }
    }

    return y + bar_h;
}

/* ── Drawing: Log form ─────────────────────────────────────────────────── */

static int PaintLogForm(HDC hdc, int y_start, int w)
{
    int cw = g_state.char_w;
    int ch = g_state.char_h;
    int row_h = ch + 8;
    int pad = cw * 2;
    int label_w = cw * 10;
    int form_h = row_h * 9 + ch + 10;
    int focused_form = !g_state.qso_list_focused && !g_state.search_focused;

    /* Form border */
    COLORREF border_clr = focused_form ? CLR_FORM_BORDER : CLR_DARKGRAY;
    DrawBox(hdc, 4, y_start, w - 8, form_h, border_clr);

    /* Title */
    {
        const char *title = g_state.editing_local_id[0] ? " Edit QSO " : " Log QSO ";
        COLORREF title_clr = g_state.editing_local_id[0] ? CLR_ORANGE : CLR_CYAN;
        DrawText_A_BG(hdc, pad, y_start, title_clr, CLR_BG, title);
    }

    int y = y_start + ch + 4;

    /* Row 1: Callsign, Band, Mode */
    {
        DrawLabelWithHotkey(hdc, pad, y + 3, CLR_LABEL, "Callsign", FieldHotkey(FIELD_CALLSIGN), cw, ch);
        int fw = 14 * cw + 6, fh = ch + 4;
        DrawField(hdc, pad + label_w, y, 14,
                  g_state.callsign, g_state.cursor_pos[FIELD_CALLSIGN],
                  focused_form && g_state.focused_field == FIELD_CALLSIGN, cw, ch);
        SetRect(&g_state.field_rects[FIELD_CALLSIGN], pad + label_w, y, pad + label_w + fw, y + fh);

        int bx = pad + label_w + 14 * cw + 16;
        DrawLabelWithHotkey(hdc, bx, y + 3, CLR_LABEL, "Band", FieldHotkey(FIELD_BAND), cw, ch);
        int bfw = 8 * cw + 6;
        DrawCycleField(hdc, bx + 5 * cw, y, 8,
                       BANDS[g_state.band_idx],
                       focused_form && g_state.focused_field == FIELD_BAND, cw, ch);
        SetRect(&g_state.field_rects[FIELD_BAND], bx + 5 * cw, y, bx + 5 * cw + bfw, y + fh);

        int mx = bx + 5 * cw + 8 * cw + 16;
        DrawLabelWithHotkey(hdc, mx, y + 3, CLR_LABEL, "Mode", FieldHotkey(FIELD_MODE), cw, ch);
        DrawCycleField(hdc, mx + 5 * cw, y, 8,
                       MODES[g_state.mode_idx],
                       focused_form && g_state.focused_field == FIELD_MODE, cw, ch);
        SetRect(&g_state.field_rects[FIELD_MODE], mx + 5 * cw, y, mx + 5 * cw + bfw, y + fh);
    }
    y += row_h;

    /* Row 2: RST Sent, RST Rcvd */
    {
        DrawLabelWithHotkey(hdc, pad, y + 3, CLR_LABEL, "RST Sent", FieldHotkey(FIELD_RST_SENT), cw, ch);
        DrawField(hdc, pad + label_w, y, 6,
                  g_state.rst_sent, g_state.cursor_pos[FIELD_RST_SENT],
                  focused_form && g_state.focused_field == FIELD_RST_SENT, cw, ch);

        int rx = pad + label_w + 6 * cw + 16;
        DrawLabelWithHotkey(hdc, rx, y + 3, CLR_LABEL, "RST Rcvd", FieldHotkey(FIELD_RST_RCVD), cw, ch);
        DrawField(hdc, rx + 10 * cw, y, 6,
                  g_state.rst_rcvd, g_state.cursor_pos[FIELD_RST_RCVD],
                  focused_form && g_state.focused_field == FIELD_RST_RCVD, cw, ch);
    }
    y += row_h;

    /* Row 3: Comment */
    {
        int field_chars = (w - pad * 2 - label_w - 12) / cw;
        if (field_chars > 60) field_chars = 60;
        if (field_chars < 10) field_chars = 10;
        DrawLabelWithHotkey(hdc, pad, y + 3, CLR_LABEL, "Comment", FieldHotkey(FIELD_COMMENT), cw, ch);
        DrawField(hdc, pad + label_w, y, field_chars,
                  g_state.comment, g_state.cursor_pos[FIELD_COMMENT],
                  focused_form && g_state.focused_field == FIELD_COMMENT, cw, ch);
    }
    y += row_h;

    /* Row 4: Notes */
    {
        int field_chars = (w - pad * 2 - label_w - 12) / cw;
        if (field_chars > 60) field_chars = 60;
        if (field_chars < 10) field_chars = 10;
        DrawLabelWithHotkey(hdc, pad, y + 3, CLR_LABEL, "Notes", FieldHotkey(FIELD_NOTES), cw, ch);
        DrawField(hdc, pad + label_w, y, field_chars,
                  g_state.notes, g_state.cursor_pos[FIELD_NOTES],
                  focused_form && g_state.focused_field == FIELD_NOTES, cw, ch);
    }
    y += row_h;

    /* Row 5: Freq, Date, Time */
    {
        DrawLabelWithHotkey(hdc, pad, y + 3, CLR_LABEL, "Freq MHz", FieldHotkey(FIELD_FREQ), cw, ch);
        DrawField(hdc, pad + label_w, y, 10,
                  g_state.freq_mhz, g_state.cursor_pos[FIELD_FREQ],
                  focused_form && g_state.focused_field == FIELD_FREQ, cw, ch);

        int dx = pad + label_w + 10 * cw + 16;
        DrawLabelWithHotkey(hdc, dx, y + 3, CLR_LABEL, "Date", FieldHotkey(FIELD_DATE), cw, ch);
        DrawField(hdc, dx + 5 * cw, y, 12,
                  g_state.date, g_state.cursor_pos[FIELD_DATE],
                  focused_form && g_state.focused_field == FIELD_DATE, cw, ch);

        int tx = dx + 5 * cw + 12 * cw + 16;
        DrawLabelWithHotkey(hdc, tx, y + 3, CLR_LABEL, "Time", FieldHotkey(FIELD_TIME), cw, ch);
        DrawField(hdc, tx + 5 * cw, y, 6,
                  g_state.time_str, g_state.cursor_pos[FIELD_TIME],
                  focused_form && g_state.focused_field == FIELD_TIME, cw, ch);
    }
    y += row_h;

    /* Row 6: QSO Duration */
    {
        char dur[32];
        if (g_state.qso_timer_active) {
            ULONGLONG elapsed = GetTickCount64() - g_state.qso_started_at;
            int secs = (int)(elapsed / 1000);
            int mins = secs / 60;
            secs %= 60;
            snprintf(dur, sizeof(dur), "QSO Duration: %02d:%02d", mins, secs);
            DrawText_A(hdc, pad, y + 3, CLR_GREEN, dur);
        } else {
            DrawText_A(hdc, pad, y + 3, CLR_DARKGRAY, "QSO Duration: 00:00");
        }
    }
    y += row_h;

    /* Row 7: padding */
    y += 4;

    /* Row 8: Hint chips */
    {
        int cx = pad;
        const char *submit_label = g_state.editing_local_id[0]
                                       ? "F10 Update QSO" : "F10 Log QSO";
        DrawChip(hdc, cx, y, CLR_FOOTER_BG, CLR_FOOTER_FG, submit_label, cw, ch);
        cx += ((int)strlen(submit_label) * cw + 16);

        DrawChip(hdc, cx, y, CLR_FOOTER_BG, CLR_FOOTER_FG, "Esc Clear", cw, ch);
        cx += (9 * cw + 16);

        DrawChip(hdc, cx, y, CLR_FOOTER_BG, CLR_FOOTER_FG, "F3 QSO List", cw, ch);
        cx += (11 * cw + 16);

        DrawChip(hdc, cx, y, CLR_FOOTER_BG, CLR_FOOTER_FG, "F4 Search", cw, ch);
    }
    y += row_h;

    return y_start + form_h;
}

/* ── Advanced view tab field definitions ────────────────────────────────── */

#define ADV_TAB_COUNT 4

static const enum Field ADV_TAB_MAIN[] = {
    FIELD_CALLSIGN, FIELD_BAND, FIELD_MODE, FIELD_FREQ,
    FIELD_DATE, FIELD_TIME, FIELD_TIME_OFF, FIELD_QTH,
    FIELD_WORKED_NAME, FIELD_RST_SENT, FIELD_RST_RCVD,
    FIELD_COMMENT, FIELD_NOTES
};

static const enum Field ADV_TAB_CONTEST[] = {
    FIELD_TX_POWER, FIELD_SUBMODE, FIELD_CONTEST_ID,
    FIELD_SERIAL_SENT, FIELD_SERIAL_RCVD,
    FIELD_EXCHANGE_SENT, FIELD_EXCHANGE_RCVD
};

static const enum Field ADV_TAB_TECHNICAL[] = {
    FIELD_PROP_MODE, FIELD_SAT_NAME, FIELD_SAT_MODE
};

static const enum Field ADV_TAB_AWARDS[] = {
    FIELD_IOTA, FIELD_ARRL_SECTION, FIELD_WORKED_STATE, FIELD_WORKED_COUNTY,
    FIELD_SKCC
};

static const enum Field *ADV_TABS[] = {
    ADV_TAB_MAIN, ADV_TAB_CONTEST, ADV_TAB_TECHNICAL, ADV_TAB_AWARDS
};

static const int ADV_TAB_COUNTS[] = {
    sizeof(ADV_TAB_MAIN) / sizeof(ADV_TAB_MAIN[0]),
    sizeof(ADV_TAB_CONTEST) / sizeof(ADV_TAB_CONTEST[0]),
    sizeof(ADV_TAB_TECHNICAL) / sizeof(ADV_TAB_TECHNICAL[0]),
    sizeof(ADV_TAB_AWARDS) / sizeof(ADV_TAB_AWARDS[0])
};

static const char *ADV_TAB_NAMES[] = { "Main", "Contest", "Technical", "Awards" };

static const enum Field *AdvTabFields(int tab, int *count)
{
    *count = ADV_TAB_COUNTS[tab];
    return ADV_TABS[tab];
}

static int AdvTabFieldIndex(int tab, enum Field f)
{
    int i, cnt;
    const enum Field *fields = AdvTabFields(tab, &cnt);
    for (i = 0; i < cnt; i++) {
        if (fields[i] == f) return i;
    }
    return -1;
}

static int CountAdvancedRows(const enum Field *fields, int count)
{
    int rows = 0;
    int i;
    for (i = 0; i < count; ) {
        if (fields[i] == FIELD_COMMENT || fields[i] == FIELD_NOTES) {
            rows++; i++;
        } else {
            rows++; i++;
            if (i < count && fields[i] != FIELD_COMMENT &&
                fields[i] != FIELD_NOTES) {
                i++; /* paired into same row */
            }
        }
    }
    return rows;
}

/* ── Drawing: Advanced log form ────────────────────────────────────────── */

static int PaintAdvancedForm(HDC hdc, int y_start, int w)
{
    int cw = g_state.char_w;
    int ch = g_state.char_h;
    int row_h = ch + 8;
    int pad = cw * 2;
    int label_w = cw * 14;
    int focused_form = !g_state.qso_list_focused && !g_state.search_focused;
    int tab = g_state.advanced_tab;
    int field_count, num_rows, form_h, y, i, t;
    const enum Field *fields;
    COLORREF border_clr;

    fields = AdvTabFields(tab, &field_count);
    num_rows = CountAdvancedRows(fields, field_count);
    /* form_h: title + tab bar + field rows + padding + hint row + margin */
    form_h = ch + 4 + row_h * (num_rows + 2) + 12;

    border_clr = focused_form ? CLR_MAGENTA : CLR_DARKGRAY;
    DrawBox(hdc, 4, y_start, w - 8, form_h, border_clr);

    /* Title */
    {
        char title[64];
        const char *pfx = g_state.editing_local_id[0] ? "Edit" : "Advanced";
        snprintf(title, sizeof(title), " %s - %s ", pfx, ADV_TAB_NAMES[tab]);
        DrawText_A_BG(hdc, pad, y_start, CLR_MAGENTA, CLR_BG, title);
    }

    y = y_start + ch + 4;

    /* Tab bar */
    {
        int tx = pad;
        for (t = 0; t < ADV_TAB_COUNT; t++) {
            char tab_label[32];
            COLORREF bg = (t == tab) ? CLR_CYAN : CLR_DARKGRAY;
            COLORREF fg = (t == tab) ? CLR_BG : CLR_WHITE;
            snprintf(tab_label, sizeof(tab_label), " %s ", ADV_TAB_NAMES[t]);
            DrawChip(hdc, tx, y, bg, fg, tab_label, cw, ch);
            tx += (int)strlen(tab_label) * cw + 12;
        }
    }
    y += row_h;

    /* Two-column field layout */
    {
        int col_w = (w - pad * 2 - 16) / 2;
        int field_w = (col_w - label_w - 8) / cw;
        if (field_w > 30) field_w = 30;
        if (field_w < 8) field_w = 8;

        for (i = 0; i < field_count; ) {
            enum Field f1 = fields[i];
            int full_w = (f1 == FIELD_COMMENT || f1 == FIELD_NOTES);

            /* First field */
            DrawLabelWithHotkey(hdc, pad, y + 3, CLR_CYAN, FIELD_LABELS[f1],
                                FieldHotkey(f1), cw, ch);
            if (f1 == FIELD_BAND) {
                DrawCycleField(hdc, pad + label_w, y, 8,
                               BANDS[g_state.band_idx],
                               focused_form && g_state.focused_field == f1,
                               cw, ch);
            } else if (f1 == FIELD_MODE) {
                DrawCycleField(hdc, pad + label_w, y, 8,
                               MODES[g_state.mode_idx],
                               focused_form && g_state.focused_field == f1,
                               cw, ch);
            } else {
                int fw = full_w ? ((w - pad * 2 - label_w - 12) / cw)
                                : field_w;
                char *buf = FieldBuffer(f1);
                if (fw > 60) fw = 60;
                if (fw < 8) fw = 8;
                DrawField(hdc, pad + label_w, y, fw,
                          buf ? buf : "", g_state.cursor_pos[f1],
                          focused_form && g_state.focused_field == f1,
                          cw, ch);
            }
            i++;

            /* Second field in same row (if first wasn't full-width) */
            if (!full_w && i < field_count) {
                enum Field f2 = fields[i];
                if (f2 != FIELD_COMMENT && f2 != FIELD_NOTES) {
                    int col2_x = pad + col_w + 8;
                    DrawLabelWithHotkey(hdc, col2_x, y + 3, CLR_CYAN,
                                       FIELD_LABELS[f2], FieldHotkey(f2), cw, ch);
                    if (f2 == FIELD_BAND) {
                        DrawCycleField(hdc, col2_x + label_w, y, 8,
                                       BANDS[g_state.band_idx],
                                       focused_form &&
                                           g_state.focused_field == f2,
                                       cw, ch);
                    } else if (f2 == FIELD_MODE) {
                        DrawCycleField(hdc, col2_x + label_w, y, 8,
                                       MODES[g_state.mode_idx],
                                       focused_form &&
                                           g_state.focused_field == f2,
                                       cw, ch);
                    } else {
                        char *buf2 = FieldBuffer(f2);
                        DrawField(hdc, col2_x + label_w, y, field_w,
                                  buf2 ? buf2 : "", g_state.cursor_pos[f2],
                                  focused_form &&
                                      g_state.focused_field == f2,
                                  cw, ch);
                    }
                    i++;
                }
            }

            y += row_h;
        }
    }

    y += 4;

    /* Hint chips */
    {
        int cx = pad;
        const char *submit_label = g_state.editing_local_id[0]
                                       ? "F10 Update QSO" : "F10 Log QSO";
        DrawChip(hdc, cx, y, CLR_FOOTER_BG, CLR_FOOTER_FG, submit_label,
                 cw, ch);
        cx += ((int)strlen(submit_label) * cw + 16);

        DrawChip(hdc, cx, y, CLR_FOOTER_BG, CLR_FOOTER_FG, "Esc Basic View",
                 cw, ch);
        cx += (14 * cw + 16);

        DrawChip(hdc, cx, y, CLR_FOOTER_BG, CLR_FOOTER_FG, "F5/F6 Tabs",
                 cw, ch);
        cx += (10 * cw + 16);

        DrawChip(hdc, cx, y, CLR_FOOTER_BG, CLR_FOOTER_FG, "F3 QSO List",
                 cw, ch);
    }

    return y_start + form_h;
}

/* ── Drawing: Lookup panel─────────────────────────────────────────────── */

static int PaintLookup(HDC hdc, int y_start, int w)
{
    int cw = g_state.char_w;
    int ch = g_state.char_h;
    int panel_h = ch * 5 + 8;
    int pad = cw * 2;

    DrawBox(hdc, 4, y_start, w - 8, panel_h, CLR_CYAN);
    DrawText_A_BG(hdc, pad, y_start, CLR_CYAN, CLR_BG, " Callsign Lookup ");

    int y = y_start + ch + 4;

    if (g_state.has_lookup) {
        char line[128];

        SelectObject(hdc, g_state.hFontBold);
        DrawText_A(hdc, pad + cw, y, CLR_TEXT, g_state.lookup_name);
        SelectObject(hdc, g_state.hFont);
        y += ch + 2;

        snprintf(line, sizeof(line), "QTH: %s", g_state.lookup_qth);
        DrawText_A(hdc, pad + cw, y, CLR_GRAY, line);

        if (g_state.lookup_grid[0]) {
            int gx = pad + cw + (int)(strlen(line) + 2) * cw;
            snprintf(line, sizeof(line), "Grid: %s", g_state.lookup_grid);
            DrawText_A(hdc, gx, y, CLR_GRAY, line);
        }
        y += ch + 2;

        snprintf(line, sizeof(line), "Country: %s   CQ Zone: %d",
                  g_state.lookup_country, g_state.lookup_cq_zone);
        DrawText_A(hdc, pad + cw, y, CLR_GRAY, line);
    } else if (g_state.lookup_in_progress) {
        static const char *dots[] = { ".", "..", "..." };
        int frame = (int)(GetTickCount64() / 400) % 3;
        char line[64];
        snprintf(line, sizeof(line), "Looking up%s", dots[frame]);
        DrawText_A(hdc, pad + cw, y + ch, CLR_CYAN, line);
    } else {
        DrawText_A(hdc, pad + cw, y + ch, CLR_DARKGRAY, "");
    }

    return y_start + panel_h + 2;
}

/* ── Drawing: QSOs table ─────────────────────────────────────────────────── */

static int PaintRecentQsos(HDC hdc, int y_start, int w, int bottom)
{
    int cw = g_state.char_w;
    int ch = g_state.char_h;
    int lcw = g_state.list_cw;
    int lch = g_state.list_ch;
    int pad = cw * 2;
    int list_row_h = lch + 3;
    int panel_h = bottom - y_start - list_row_h - 4;
    if (panel_h < list_row_h * 3) panel_h = list_row_h * 3;

    /* Store layout metrics for click and double-click detection */
    g_state.qso_list_y = y_start;
    g_state.qso_list_row_h = list_row_h;

    /* Border color depends on focus state */
    COLORREF border_clr = CLR_CYAN;
    if (g_state.search_focused)
        border_clr = CLR_MAGENTA;
    else if (g_state.qso_list_focused)
        border_clr = CLR_GREEN;

    DrawBox(hdc, 4, y_start, w - 8, panel_h, border_clr);

    /* Title with total count (drawn using main font) */
    {
        char title[64];
        if (g_state.qso_loading)
            snprintf(title, sizeof(title), " QSOs (loading...) ");
        else if (g_state.search_focused)
            snprintf(title, sizeof(title), " Search QSOs (%d) ", g_state.recent_count);
        else if (g_state.qso_list_focused)
            snprintf(title, sizeof(title), " QSOs (%d) (focused) ", g_state.recent_count);
        else
            snprintf(title, sizeof(title), " QSOs (%d) ", g_state.recent_count);
        DrawText_A_BG(hdc, pad, y_start, border_clr, CLR_BG, title);
    }

    int y = y_start + ch + 4;

    /* Search bar (if search focused) — uses main font */
    if (g_state.search_focused) {
        DrawText_A(hdc, pad + cw, y + 1, CLR_CYAN, "Search:");
        DrawField(hdc, pad + cw + 8 * cw, y, 30,
                  g_state.search_text, g_state.search_cursor, 1, cw, ch);
        y += (ch + 4) + 2;
    }

    /* Switch to small font for table header and rows */
    SelectObject(hdc, g_state.hFontSmallBold);

    /* Table header */
    {
        char hdr[256];
        snprintf(hdr, sizeof(hdr),
                  "%-19s %-10s %-5s %-5s %-4s %-4s %-16s %-6s",
                  "UTC", "Callsign", "Band", "Mode",
                  "Sent", "Rcvd", "Country", "Grid");
        DrawText_A(hdc, pad + lcw, y + 1, CLR_HIGHLIGHT, hdr);
        SelectObject(hdc, g_state.hFontSmall);
        y += list_row_h;
        DrawHLine(hdc, pad, w - pad, y, CLR_DARKGRAY);
        y += 2;
    }

    /* Store first-row y for hit-testing */
    int rows_start_y = y;
    (void)rows_start_y;

    /* Rows */
    int max_rows = (y_start + panel_h - y) / list_row_h;
    if (max_rows < 1) max_rows = 1;
    g_state.qso_page_size = max_rows;

    /* Adjust scroll to keep selection visible */
    if (g_state.qso_selected >= 0) {
        if (g_state.qso_selected < g_state.qso_scroll)
            g_state.qso_scroll = g_state.qso_selected;
        if (g_state.qso_selected >= g_state.qso_scroll + max_rows)
            g_state.qso_scroll = g_state.qso_selected - max_rows + 1;
    }
    if (g_state.qso_scroll < 0) g_state.qso_scroll = 0;

    for (int i = 0; i < max_rows && (g_state.qso_scroll + i) < g_state.recent_count; i++) {
        int idx = g_state.qso_scroll + i;
        RecentQso *q = &g_state.recent_qsos[idx];

        /* Filter by search text */
        if (g_state.search_text[0]) {
            char upper_call[16], upper_search[64];
            int j;
            for (j = 0; q->callsign[j]; j++)
                upper_call[j] = (char)toupper((unsigned char)q->callsign[j]);
            upper_call[j] = 0;
            for (j = 0; g_state.search_text[j]; j++)
                upper_search[j] = (char)toupper((unsigned char)g_state.search_text[j]);
            upper_search[j] = 0;
            if (!strstr(upper_call, upper_search) &&
                !strstr(q->country, g_state.search_text))
                continue;
        }

        int selected = (idx == g_state.qso_selected) && g_state.qso_list_focused;

        char row[256];
        snprintf(row, sizeof(row),
                  "%-19s %-10s %-5s %-5s %-4s %-4s %-16s %-6s",
                  q->utc, q->callsign, q->band, q->mode,
                  q->rst_sent, q->rst_rcvd, q->country, q->grid);

        if (selected) {
            FillRect_Color(hdc, pad, y, w - pad * 2, list_row_h, CLR_HIGHLIGHT);
            DrawText_A_BG(hdc, pad + lcw, y + 2, CLR_HILITE_FG, CLR_HIGHLIGHT, row);
        } else {
            char utc_part[24];
            snprintf(utc_part, sizeof(utc_part), "%-19s ", q->utc);
            DrawText_A(hdc, pad + lcw, y + 2, CLR_TEXT, utc_part);

            int call_x = pad + lcw + 20 * lcw;
            char call_part[16];
            snprintf(call_part, sizeof(call_part), "%-10s ", q->callsign);
            SelectObject(hdc, g_state.hFontSmallBold);
            DrawText_A(hdc, call_x, y + 2, CLR_HIGHLIGHT, call_part);
            SelectObject(hdc, g_state.hFontSmall);

            int rest_x = call_x + 11 * lcw;
            char rest_part[128];
            snprintf(rest_part, sizeof(rest_part),
                      "%-5s %-5s %-4s %-4s %-16s %-6s",
                      q->band, q->mode, q->rst_sent, q->rst_rcvd,
                      q->country, q->grid);
            DrawText_A(hdc, rest_x, y + 2, CLR_TEXT, rest_part);
        }

        y += list_row_h;
    }

    /* Restore main font */
    SelectObject(hdc, g_state.hFont);

    if (g_state.recent_count == 0 && !g_state.qso_loading) {
        DrawText_A(hdc, pad + cw, y + 2, CLR_DARKGRAY, "No QSOs logged yet");
    }

    return y_start + panel_h;
}

/* ── Drawing: Footer ───────────────────────────────────────────────────── */

static void PaintFooter(HDC hdc, int y, int w)
{
    int cw = g_state.char_w;
    int ch = g_state.char_h;
    int bar_h = ch + 4;

    FillRect_Color(hdc, 0, y, w, bar_h, CLR_BG);
    DrawHLine(hdc, 0, w, y, CLR_CYAN);

    int x = cw;
    y += 2;

    const char *shortcuts[] = {
        "Ctrl+Q Quit", "F1 Help", "F2 Advanced", "F10 Log", "Tab Next",
        "F3 List", "F4 Search", "F7 Timer", "F8 Rig", NULL
    };
    for (int i = 0; shortcuts[i]; i++) {
        DrawChip(hdc, x, y, CLR_FOOTER_BG, CLR_FOOTER_FG, shortcuts[i], cw, ch);
        x += (int)strlen(shortcuts[i]) * cw + 14;
        if (x > w - 10 * cw) break;
    }
}

/* ── Drawing: Help overlay ─────────────────────────────────────────────── */

static void PaintHelp(HDC hdc, int w, int h)
{
    int cw = g_state.char_w;
    int ch = g_state.char_h;

    int ow = cw * 52;
    int oh = ch * 24;
    int ox = (w - ow) / 2;
    int oy = (h - oh) / 2;

    int header_h = ch * 3;

    /* Body */
    FillRect_Color(hdc, ox, oy + header_h, ow, oh - header_h, CLR_BG);

    /* Navy header band */
    FillRect_Color(hdc, ox, oy, ow, header_h, CLR_HEADER_BG);
    DrawBox(hdc, ox, oy, ow, oh, CLR_HEADER_BG);

    SelectObject(hdc, g_state.hFontBold);
    DrawText_A(hdc, ox + cw * 2, oy + ch, CLR_HEADER_FG, "QsoRipper Help");
    SelectObject(hdc, g_state.hFont);

    /* Divider between header and body */
    DrawHLine(hdc, ox, ox + ow, oy + header_h, CLR_CYAN);

    /* Outer border */
    DrawBox(hdc, ox, oy, ow, oh, CLR_CYAN);

    int y = oy + header_h + ch;
    const char *lines[] = {
        "Ctrl+Q          Quit application",
        "F1              Toggle this help",
        "F2              Toggle advanced view",
        "F3              Toggle QSO list focus",
        "F4              Toggle search",
        "F7              Reset QSO timer",
        "F8              Toggle rig control",
        "F5 / F6         Adv tab next/prev",
        "F10 / Alt+Enter Log QSO (or update)",
        "Tab / Shift+Tab Navigate fields",
        "Left / Right    Cycle Band/Mode",
        "Esc             Clear form / exit focus",
        "Backspace       Delete character",
        "Alt+C/B/M/S/R   Jump to field",
        "Alt+O/N/Z/D/T   Jump to field",
        "Up / Down       Navigate QSO list",
        "Enter           Load selected QSO",
        "D / Delete      Delete selected QSO",
        "",
        "      Press F1 or Esc to close",
        NULL
    };
    for (int i = 0; lines[i]; i++) {
        DrawText_A(hdc, ox + cw * 2, y, CLR_TEXT, lines[i]);
        y += ch + 2;
    }
}

/* ── Drawing: Delete confirmation dialog ───────────────────────────────── */

static void PaintConfirmDelete(HDC hdc, int w, int h)
{
    int cw = g_state.char_w;
    int ch = g_state.char_h;

    int ow = cw * 40;
    int oh = ch * 8;
    int ox = (w - ow) / 2;
    int oy = (h - oh) / 2;

    FillRect_Color(hdc, ox, oy, ow, oh, RGB(40, 0, 0));
    DrawBox(hdc, ox, oy, ow, oh, CLR_RED);

    SelectObject(hdc, g_state.hFontBold);
    DrawText_A(hdc, ox + cw * 2, oy + ch, CLR_RED, "Delete QSO?");
    SelectObject(hdc, g_state.hFont);

    if (g_state.qso_selected >= 0 && g_state.qso_selected < g_state.recent_count) {
        char msg[128];
        snprintf(msg, sizeof(msg), "Delete QSO with %s?",
                  g_state.recent_qsos[g_state.qso_selected].callsign);
        DrawText_A(hdc, ox + cw * 2, oy + ch * 3, CLR_WHITE, msg);
    }

    DrawText_A(hdc, ox + cw * 2, oy + ch * 5, CLR_HEADER_FG,
               "Y = confirm   N/Esc = cancel");
}

/* ── Main paint routine (double-buffered) ──────────────────────────────── */

static void PaintAll(HWND hwnd, HDC hdc_screen, RECT *rc)
{
    int w = rc->right - rc->left;
    int h = rc->bottom - rc->top;
    int ch = g_state.char_h;

    /* Create back buffer */
    HDC hdc = CreateCompatibleDC(hdc_screen);
    HBITMAP hbm = CreateCompatibleBitmap(hdc_screen, w, h);
    HBITMAP oldBm = (HBITMAP)SelectObject(hdc, hbm);
    HFONT oldFont = (HFONT)SelectObject(hdc, g_state.hFont);

    /* Clear background */
    FillRect_Color(hdc, 0, 0, w, h, CLR_BG);

    int y = 0;

    /* Header (3 rows) */
    PaintHeader(hdc, rc);
    y = ch * 3 + 4;

    /* Status bar */
    y = PaintStatus(hdc, y, w);

    /* Log form */
    if (g_state.advanced_view)
        y = PaintAdvancedForm(hdc, y, w);
    else
        y = PaintLogForm(hdc, y, w);
    y += 2;

    /* Lookup panel */
    y = PaintLookup(hdc, y, w);
    y += 2;

    /* Footer position */
    int footer_y = h - ch - 6;

    /* Recent QSOs (fill between lookup and footer) */
    PaintRecentQsos(hdc, y, w, footer_y);

    /* Footer */
    PaintFooter(hdc, footer_y, w);

    /* Overlays */
    if (g_state.help_visible) {
        PaintHelp(hdc, w, h);
    }
    if (g_state.confirm_delete_visible) {
        PaintConfirmDelete(hdc, w, h);
    }

    /* Blit to screen */
    BitBlt(hdc_screen, 0, 0, w, h, hdc, 0, 0, SRCCOPY);

    SelectObject(hdc, oldFont);
    SelectObject(hdc, oldBm);
    DeleteObject(hbm);
    DeleteDC(hdc);

    (void)hwnd;
}

/* ── Keyboard: field editing ───────────────────────────────────────────── */

static void InsertChar(enum Field f, char c)
{
    if (g_state.field_all_selected) {
        char *clrbuf = FieldBuffer(f);
        if (clrbuf) clrbuf[0] = '\0';
        g_state.cursor_pos[f] = 0;
        g_state.field_all_selected = 0;
    }

    char *buf = FieldBuffer(f);
    if (!buf) return;
    int maxlen = FieldMaxLen(f);
    int len = (int)strlen(buf);
    if (len >= maxlen - 1) return;

    int pos = g_state.cursor_pos[f];
    if (pos > len) pos = len;

    /* Shift right */
    memmove(buf + pos + 1, buf + pos, (size_t)(len - pos + 1));
    buf[pos] = c;
    g_state.cursor_pos[f] = pos + 1;

    /* Auto-uppercase callsign */
    if (f == FIELD_CALLSIGN) {
        for (int i = 0; buf[i]; i++)
            buf[i] = (char)toupper((unsigned char)buf[i]);
    }
}

static void DeleteChar(enum Field f)
{
    if (g_state.field_all_selected) {
        char *clrbuf = FieldBuffer(f);
        if (clrbuf) clrbuf[0] = '\0';
        g_state.cursor_pos[f] = 0;
        g_state.field_all_selected = 0;
        return;
    }

    char *buf = FieldBuffer(f);
    if (!buf) return;
    int pos = g_state.cursor_pos[f];
    if (pos <= 0) return;
    int len = (int)strlen(buf);
    memmove(buf + pos - 1, buf + pos, (size_t)(len - pos + 1));
    g_state.cursor_pos[f] = pos - 1;
}

/* ── Keyboard: Band/Mode type-select ───────────────────────────────────── */

static void TypeSelectBand(char c)
{
    c = (char)toupper((unsigned char)c);
    /* Find next band starting with this char after current */
    for (int attempt = 0; attempt < NUM_BANDS; attempt++) {
        int idx = (g_state.band_idx + 1 + attempt) % NUM_BANDS;
        if (toupper((unsigned char)BANDS[idx][0]) == c) {
            g_state.band_idx = idx;
            snprintf(g_state.freq_mhz, sizeof(g_state.freq_mhz),
                      "%.3f", BAND_DEFAULT_FREQS[idx]);
            g_state.cursor_pos[FIELD_FREQ] = (int)strlen(g_state.freq_mhz);
            return;
        }
    }
}

static void TypeSelectMode(char c)
{
    c = (char)toupper((unsigned char)c);
    for (int attempt = 0; attempt < NUM_MODES; attempt++) {
        int idx = (g_state.mode_idx + 1 + attempt) % NUM_MODES;
        if (toupper((unsigned char)MODES[idx][0]) == c) {
            g_state.mode_idx = idx;
            ApplyModeDefaults();
            return;
        }
    }
}

/* ── Keyboard: main handler ────────────────────────────────────────────── */

static void OnKeyDown(HWND hwnd, WPARAM vk, LPARAM lp)
{
    int alt_down = (GetKeyState(VK_MENU) & 0x8000) != 0;
    int ctrl_down = (GetKeyState(VK_CONTROL) & 0x8000) != 0;
    int shift_down = (GetKeyState(VK_SHIFT) & 0x8000) != 0;

    (void)lp;

    /* ── Confirm delete dialog ───────────────────────────────────── */
    if (g_state.confirm_delete_visible) {
        if (vk == 'Y') {
            DeleteSelectedQso();
        } else {
            g_state.confirm_delete_visible = 0;
        }
        InvalidateRect(hwnd, NULL, FALSE);
        return;
    }

    /* ── Help overlay ────────────────────────────────────────────── */
    if (g_state.help_visible) {
        if (vk == VK_F1 || vk == VK_ESCAPE) {
            g_state.help_visible = 0;
            InvalidateRect(hwnd, NULL, FALSE);
        }
        return;
    }

    /* ── Global shortcuts ────────────────────────────────────────── */

    /* Ctrl+Q: quit */
    if (ctrl_down && vk == 'Q') {
        PostQuitMessage(0);
        return;
    }

    /* F1: help */
    if (vk == VK_F1) {
        g_state.help_visible = !g_state.help_visible;
        InvalidateRect(hwnd, NULL, FALSE);
        return;
    }

    /* F2: toggle advanced view */
    if (vk == VK_F2) {
        g_state.advanced_view = !g_state.advanced_view;
        if (g_state.advanced_view) {
            /* Keep field if it exists in current tab, else focus first */
            if (AdvTabFieldIndex(g_state.advanced_tab,
                                 g_state.focused_field) < 0) {
                int cnt;
                const enum Field *flds =
                    AdvTabFields(g_state.advanced_tab, &cnt);
                SetFocusField(flds[0]);
            }
        } else {
            /* Return to basic: ensure field is in basic range */
            if (g_state.focused_field > FIELD_TIME)
                SetFocusField(FIELD_CALLSIGN);
        }
        g_state.qso_list_focused = 0;
        g_state.search_focused = 0;
        InvalidateRect(hwnd, NULL, FALSE);
        return;
    }

    /* F5: next advanced tab */
    if (vk == VK_F5 && g_state.advanced_view) {
        int cnt;
        const enum Field *flds;
        g_state.advanced_tab = (g_state.advanced_tab + 1) % ADV_TAB_COUNT;
        flds = AdvTabFields(g_state.advanced_tab, &cnt);
        SetFocusField(flds[0]);
        InvalidateRect(hwnd, NULL, FALSE);
        return;
    }

    /* F6: previous advanced tab */
    if (vk == VK_F6 && g_state.advanced_view) {
        int cnt;
        const enum Field *flds;
        g_state.advanced_tab =
            (g_state.advanced_tab + ADV_TAB_COUNT - 1) % ADV_TAB_COUNT;
        flds = AdvTabFields(g_state.advanced_tab, &cnt);
        SetFocusField(flds[0]);
        InvalidateRect(hwnd, NULL, FALSE);
        return;
    }

    /* F3: toggle QSO list focus */
    if (vk == VK_F3) {
        g_state.search_focused = 0;
        g_state.qso_list_focused = !g_state.qso_list_focused;
        if (g_state.qso_list_focused && g_state.qso_selected < 0 &&
            g_state.recent_count > 0) {
            g_state.qso_selected = 0;
        }
        InvalidateRect(hwnd, NULL, FALSE);
        return;
    }

    /* F4: toggle search */
    if (vk == VK_F4) {
        g_state.search_focused = !g_state.search_focused;
        if (g_state.search_focused) {
            g_state.qso_list_focused = 1;
        }
        InvalidateRect(hwnd, NULL, FALSE);
        return;
    }

    /* F7: reset QSO timer and update Time field to now */
    if (vk == VK_F7) {
        g_state.qso_timer_active = 1;
        g_state.qso_started_at = GetTickCount64();
        SetCurrentDateTime();
        InvalidateRect(hwnd, NULL, FALSE);
        return;
    }

    /* F8: toggle rig control */
    if (vk == VK_F8) {
        g_state.rig_enabled = !g_state.rig_enabled;
        if (!g_state.rig_enabled) {
            g_state.rig_connected = 0;
            g_state.rig_freq_display[0] = 0;
            g_state.rig_freq_mhz[0] = 0;
            g_state.rig_band[0] = 0;
            g_state.rig_mode[0] = 0;
            SetStatus("Rig control OFF", 0);
        } else {
            g_state.last_rig_poll = 0;
            SetStatus("Rig control ON", 0);
        }
        InvalidateRect(hwnd, NULL, FALSE);
        return;
    }

    /* F10, Shift+Enter, or Alt+Enter: log/update QSO */
    if (vk == VK_F10 || (shift_down && vk == VK_RETURN) || (alt_down && vk == VK_RETURN)) {
        LogQso();
        InvalidateRect(hwnd, NULL, FALSE);
        return;
    }

    /* ── Alt+key: jump to field ──────────────────────────────────── */
    if (alt_down && !ctrl_down) {
        enum Field target = FIELD_COUNT;
        switch (vk) {
        case 'C': target = FIELD_CALLSIGN; break;
        case 'B': target = FIELD_BAND; break;
        case 'M': target = FIELD_MODE; break;
        case 'S': target = FIELD_RST_SENT; break;
        case 'R': target = FIELD_RST_RCVD; break;
        case 'O': target = FIELD_COMMENT; break;
        case 'N': target = FIELD_NOTES; break;
        case 'Z': target = FIELD_FREQ; break;
        case 'D': target = FIELD_DATE; break;
        case 'T': target = FIELD_TIME; break;
        case 'A': target = FIELD_WORKED_NAME; break;
        case 'I': target = FIELD_TIME_OFF; break;
        case 'Q': target = FIELD_QTH; break;
        case 'W': target = FIELD_TX_POWER; break;
        case 'U': target = FIELD_SUBMODE; break;
        case 'E': target = FIELD_SERIAL_SENT; break;
        case 'P': target = FIELD_PROP_MODE; break;
        case 'K': target = FIELD_SKCC; break;
        }
        if (target != FIELD_COUNT) {
            SetFocusField(target);
            g_state.qso_list_focused = 0;
            g_state.search_focused = 0;
            InvalidateRect(hwnd, NULL, FALSE);
            return;
        }
    }

    /* ── Search mode ─────────────────────────────────────────────── */
    if (g_state.search_focused) {
        if (vk == VK_ESCAPE) {
            g_state.search_focused = 0;
            g_state.search_text[0] = 0;
            g_state.search_cursor = 0;
            InvalidateRect(hwnd, NULL, FALSE);
            return;
        }
        if (vk == VK_BACK) {
            if (g_state.search_cursor > 0) {
                int len = (int)strlen(g_state.search_text);
                memmove(g_state.search_text + g_state.search_cursor - 1,
                        g_state.search_text + g_state.search_cursor,
                        (size_t)(len - g_state.search_cursor + 1));
                g_state.search_cursor--;
            }
            InvalidateRect(hwnd, NULL, FALSE);
            return;
        }
        if (vk == VK_LEFT) {
            if (g_state.search_cursor > 0) g_state.search_cursor--;
            InvalidateRect(hwnd, NULL, FALSE);
            return;
        }
        if (vk == VK_RIGHT) {
            if (g_state.search_cursor < (int)strlen(g_state.search_text))
                g_state.search_cursor++;
            InvalidateRect(hwnd, NULL, FALSE);
            return;
        }
        /* Tab exits search to list */
        if (vk == VK_TAB) {
            g_state.search_focused = 0;
            g_state.qso_list_focused = 1;
            InvalidateRect(hwnd, NULL, FALSE);
            return;
        }
        /* Other keys handled in OnChar */
        return;
    }

    /* ── QSO list navigation ─────────────────────────────────────── */
    if (g_state.qso_list_focused) {
        if (vk == VK_ESCAPE) {
            g_state.qso_list_focused = 0;
            g_state.qso_selected = -1;
            InvalidateRect(hwnd, NULL, FALSE);
            return;
        }
        if (vk == VK_UP) {
            if (g_state.qso_selected > 0)
                g_state.qso_selected--;
            else if (g_state.recent_count > 0)
                g_state.qso_selected = 0;
            InvalidateRect(hwnd, NULL, FALSE);
            return;
        }
        if (vk == VK_DOWN) {
            if (g_state.qso_selected < g_state.recent_count - 1)
                g_state.qso_selected++;
            InvalidateRect(hwnd, NULL, FALSE);
            return;
        }
        if (vk == VK_HOME && ctrl_down) {
            g_state.qso_selected = g_state.recent_count > 0 ? 0 : -1;
            g_state.qso_scroll = 0;
            InvalidateRect(hwnd, NULL, FALSE);
            return;
        }
        if (vk == VK_END && ctrl_down) {
            g_state.qso_selected = g_state.recent_count - 1;
            InvalidateRect(hwnd, NULL, FALSE);
            return;
        }
        if (vk == VK_PRIOR) { /* Page Up */
            int page = g_state.qso_page_size > 0 ? g_state.qso_page_size : 10;
            g_state.qso_selected -= page;
            if (g_state.qso_selected < 0) g_state.qso_selected = 0;
            InvalidateRect(hwnd, NULL, FALSE);
            return;
        }
        if (vk == VK_NEXT) { /* Page Down */
            int page = g_state.qso_page_size > 0 ? g_state.qso_page_size : 10;
            g_state.qso_selected += page;
            if (g_state.qso_selected >= g_state.recent_count)
                g_state.qso_selected = g_state.recent_count - 1;
            InvalidateRect(hwnd, NULL, FALSE);
            return;
        }
        if (vk == VK_RETURN) {
            LoadSelectedQso();
            InvalidateRect(hwnd, NULL, FALSE);
            return;
        }
        if (vk == 'D' || vk == VK_DELETE) {
            if (g_state.qso_selected >= 0) {
                g_state.confirm_delete_visible = 1;
                InvalidateRect(hwnd, NULL, FALSE);
            }
            return;
        }
        if (vk == VK_TAB) {
            g_state.qso_list_focused = 0;
            SetFocusField(FIELD_CALLSIGN);
            InvalidateRect(hwnd, NULL, FALSE);
            return;
        }
        return;
    }

    /* ── Form navigation ─────────────────────────────────────────── */

    /* Escape: in advanced view return to basic, otherwise clear form */
    if (vk == VK_ESCAPE) {
        if (g_state.advanced_view) {
            g_state.advanced_view = 0;
            if (g_state.focused_field > FIELD_TIME)
                SetFocusField(FIELD_CALLSIGN);
        } else {
            ClearForm();
        }
        InvalidateRect(hwnd, NULL, FALSE);
        return;
    }

    /* Tab: next field */
    if (vk == VK_TAB) {
        if (g_state.advanced_view) {
            /* Navigate within current tab's fields */
            int cnt;
            const enum Field *flds =
                AdvTabFields(g_state.advanced_tab, &cnt);
            int idx = AdvTabFieldIndex(g_state.advanced_tab,
                                       g_state.focused_field);
            if (idx < 0) idx = 0;
            if (shift_down)
                idx = (idx + cnt - 1) % cnt;
            else
                idx = (idx + 1) % cnt;
            SetFocusField(flds[idx]);
        } else {
            /* Basic view: cycle through basic fields only */
            enum Field nf;
            if (shift_down) {
                nf = (g_state.focused_field > FIELD_CALLSIGN)
                    ? (enum Field)(g_state.focused_field - 1)
                    : FIELD_TIME;
            } else {
                nf = (g_state.focused_field < FIELD_TIME)
                    ? (enum Field)(g_state.focused_field + 1)
                    : FIELD_CALLSIGN;
            }
            SetFocusField(nf);
        }
        InvalidateRect(hwnd, NULL, FALSE);
        return;
    }

    /* Left/Right: cycle for band/mode, cursor move for text fields */
    if (vk == VK_LEFT) {
        if (g_state.focused_field == FIELD_BAND) {
            g_state.band_idx = (g_state.band_idx + NUM_BANDS - 1) % NUM_BANDS;
            snprintf(g_state.freq_mhz, sizeof(g_state.freq_mhz),
                      "%.3f", BAND_DEFAULT_FREQS[g_state.band_idx]);
            g_state.cursor_pos[FIELD_FREQ] = (int)strlen(g_state.freq_mhz);
        } else if (g_state.focused_field == FIELD_MODE) {
            g_state.mode_idx = (g_state.mode_idx + NUM_MODES - 1) % NUM_MODES;
            ApplyModeDefaults();
        } else {
            enum Field f = g_state.focused_field;
            if (g_state.field_all_selected) {
                g_state.field_all_selected = 0;
                g_state.cursor_pos[f] = 0;
            } else if (g_state.cursor_pos[f] > 0) {
                g_state.cursor_pos[f]--;
            }
        }
        InvalidateRect(hwnd, NULL, FALSE);
        return;
    }
    if (vk == VK_RIGHT) {
        if (g_state.focused_field == FIELD_BAND) {
            g_state.band_idx = (g_state.band_idx + 1) % NUM_BANDS;
            snprintf(g_state.freq_mhz, sizeof(g_state.freq_mhz),
                      "%.3f", BAND_DEFAULT_FREQS[g_state.band_idx]);
            g_state.cursor_pos[FIELD_FREQ] = (int)strlen(g_state.freq_mhz);
        } else if (g_state.focused_field == FIELD_MODE) {
            g_state.mode_idx = (g_state.mode_idx + 1) % NUM_MODES;
            ApplyModeDefaults();
        } else {
            enum Field f = g_state.focused_field;
            char *buf = FieldBuffer(f);
            if (g_state.field_all_selected) {
                g_state.field_all_selected = 0;
                if (buf) g_state.cursor_pos[f] = (int)strlen(buf);
            } else if (buf && g_state.cursor_pos[f] < (int)strlen(buf)) {
                g_state.cursor_pos[f]++;
            }
        }
        InvalidateRect(hwnd, NULL, FALSE);
        return;
    }

    /* Backspace */
    if (vk == VK_BACK) {
        enum Field f = g_state.focused_field;
        if (f != FIELD_BAND && f != FIELD_MODE) {
            DeleteChar(f);

            /* Trigger lookup debounce if callsign changed */
            if (f == FIELD_CALLSIGN) {
                ClearLookupDisplay();
                g_state.last_callsign_change = GetTickCount64();
            }
        }
        InvalidateRect(hwnd, NULL, FALSE);
        return;
    }

    /* Home / End */
    if (vk == VK_HOME) {
        g_state.field_all_selected = 0;
        g_state.cursor_pos[g_state.focused_field] = 0;
        InvalidateRect(hwnd, NULL, FALSE);
        return;
    }
    if (vk == VK_END) {
        g_state.field_all_selected = 0;
        char *buf = FieldBuffer(g_state.focused_field);
        if (buf)
            g_state.cursor_pos[g_state.focused_field] = (int)strlen(buf);
        InvalidateRect(hwnd, NULL, FALSE);
        return;
    }
}

/* ── Character input ───────────────────────────────────────────────────── */

static void OnChar(HWND hwnd, WPARAM ch)
{
    /* Filter control characters */
    if (ch < 32 || ch == 127) return;

    /* Search mode */
    if (g_state.search_focused) {
        int len = (int)strlen(g_state.search_text);
        if (len < (int)sizeof(g_state.search_text) - 1) {
            int pos = g_state.search_cursor;
            memmove(g_state.search_text + pos + 1,
                    g_state.search_text + pos,
                    (size_t)(len - pos + 1));
            g_state.search_text[pos] = (char)ch;
            g_state.search_cursor++;
        }
        InvalidateRect(hwnd, NULL, FALSE);
        return;
    }

    /* QSO list: should not receive chars normally */
    if (g_state.qso_list_focused) return;

    /* Band/Mode: type-select */
    if (g_state.focused_field == FIELD_BAND) {
        TypeSelectBand((char)ch);
        InvalidateRect(hwnd, NULL, FALSE);
        return;
    }
    if (g_state.focused_field == FIELD_MODE) {
        TypeSelectMode((char)ch);
        InvalidateRect(hwnd, NULL, FALSE);
        return;
    }

    /* Normal field input */
    InsertChar(g_state.focused_field, (char)ch);

    /* Start QSO timer on first callsign character */
    if (g_state.focused_field == FIELD_CALLSIGN && !g_state.qso_timer_active) {
        g_state.qso_timer_active = 1;
        g_state.qso_started_at = GetTickCount64();
    }

    /* Callsign lookup debounce */
    if (g_state.focused_field == FIELD_CALLSIGN) {
        ClearLookupDisplay();
        g_state.last_callsign_change = GetTickCount64();
    }

    InvalidateRect(hwnd, NULL, FALSE);
}

/* ── Timer tick ────────────────────────────────────────────────────────── */

static void OnTimer(HWND hwnd)
{
    if (g_state.last_callsign_change > 0 &&
        g_state.callsign[0] != 0 &&
        strlen(g_state.callsign) >= 3) {
        ULONGLONG elapsed = GetTickCount64() - g_state.last_callsign_change;
        if (elapsed >= LOOKUP_DEBOUNCE_MS) {
            g_state.last_callsign_change = 0;
            if (!g_state.lookup_in_progress &&
                _stricmp(g_state.callsign, g_state.last_looked_up) != 0) {
                LookupThreadArg *arg = (LookupThreadArg *)malloc(sizeof(LookupThreadArg));
                if (arg) {
                    arg->hwnd = hwnd;
                    safe_strcpy(arg->callsign, sizeof(arg->callsign), g_state.callsign);
                    g_state.lookup_in_progress = 1;
                    uintptr_t h = _beginthreadex(NULL, 0, LookupThread, arg, 0, NULL);
                    if (h) CloseHandle((HANDLE)h);
                    else { free(arg); g_state.lookup_in_progress = 0; }
                }
            }
        }
    }

    /* Rig poll: every 1000ms when enabled and no poll in flight */
    if (g_state.rig_enabled && !g_state.rig_poll_in_progress) {
        ULONGLONG now = GetTickCount64();
        if (now - g_state.last_rig_poll >= 1000) {
            g_state.last_rig_poll = now;
            RigPollArg *rarg = (RigPollArg *)malloc(sizeof(RigPollArg));
            if (rarg) {
                rarg->hwnd = hwnd;
                g_state.rig_poll_in_progress = 1;
                uintptr_t rh = _beginthreadex(NULL, 0, RigPollThread, rarg, 0, NULL);
                if (rh) CloseHandle((HANDLE)rh);
                else { free(rarg); g_state.rig_poll_in_progress = 0; }
            }
        }
    }

    InvalidateRect(hwnd, NULL, FALSE);
}

/* ── Window procedure ──────────────────────────────────────────────────── */

static LRESULT CALLBACK WndProc(HWND hwnd, UINT msg, WPARAM wParam, LPARAM lParam)
{
    switch (msg) {
    case WM_CREATE:
    {
        /* Create fonts */
        HDC hdc = GetDC(hwnd);
        int dpi = GetDeviceCaps(hdc, LOGPIXELSY);
        int fontHeight = -MulDiv(FONT_SIZE, dpi, 72);
        int smallFontHeight = -MulDiv(9, dpi, 72);

        g_state.hFont = CreateFontW(
            fontHeight, 0, 0, 0, FW_NORMAL, FALSE, FALSE, FALSE,
            DEFAULT_CHARSET, OUT_DEFAULT_PRECIS, CLIP_DEFAULT_PRECIS,
            CLEARTYPE_QUALITY, FIXED_PITCH | FF_MODERN, FONT_NAME);

        g_state.hFontBold = CreateFontW(
            fontHeight, 0, 0, 0, FW_BOLD, FALSE, FALSE, FALSE,
            DEFAULT_CHARSET, OUT_DEFAULT_PRECIS, CLIP_DEFAULT_PRECIS,
            CLEARTYPE_QUALITY, FIXED_PITCH | FF_MODERN, FONT_NAME);

        g_state.hFontSmall = CreateFontW(
            smallFontHeight, 0, 0, 0, FW_NORMAL, FALSE, FALSE, FALSE,
            DEFAULT_CHARSET, OUT_DEFAULT_PRECIS, CLIP_DEFAULT_PRECIS,
            CLEARTYPE_QUALITY, FIXED_PITCH | FF_MODERN, FONT_NAME);

        g_state.hFontSmallBold = CreateFontW(
            smallFontHeight, 0, 0, 0, FW_BOLD, FALSE, FALSE, FALSE,
            DEFAULT_CHARSET, OUT_DEFAULT_PRECIS, CLIP_DEFAULT_PRECIS,
            CLEARTYPE_QUALITY, FIXED_PITCH | FF_MODERN, FONT_NAME);

        /* Measure main font character size */
        HFONT old = (HFONT)SelectObject(hdc, g_state.hFont);
        TEXTMETRICW tm;
        GetTextMetricsW(hdc, &tm);
        g_state.char_w = tm.tmAveCharWidth;
        g_state.char_h = tm.tmHeight;

        /* Measure small font character size */
        SelectObject(hdc, g_state.hFontSmall);
        GetTextMetricsW(hdc, &tm);
        g_state.list_cw = tm.tmAveCharWidth;
        g_state.list_ch = tm.tmHeight;

        SelectObject(hdc, old);
        ReleaseDC(hwnd, hdc);

        g_state.hwnd = hwnd;

        /* Start timer */
        SetTimer(hwnd, TIMER_ID, TIMER_MS, NULL);

        /* Build menu bar */
        {
            HMENU hMenuBar = CreateMenu();

            HMENU hFile = CreatePopupMenu();
            AppendMenuW(hFile, MF_STRING, IDM_FILE_EXIT, L"E&xit\tAlt+F4");

            HMENU hHelp = CreatePopupMenu();
            AppendMenuW(hHelp, MF_STRING, IDM_HELP_KEYBOARD, L"&Keyboard Shortcuts\tF1");
            AppendMenuW(hHelp, MF_STRING, IDM_HELP_ABOUT,    L"&About QsoRipper");

            AppendMenuW(hMenuBar, MF_POPUP, (UINT_PTR)hFile, L"&File");
            AppendMenuW(hMenuBar, MF_POPUP, (UINT_PTR)hHelp, L"&Help");

            SetMenu(hwnd, hMenuBar);
        }

        /* Kick off async data loads so the window appears immediately */
        RefreshQsoListAsync(hwnd);
        FetchSpaceWeather();
        break;
    }

    case WM_COMMAND:
        switch (LOWORD(wParam)) {
        case IDM_FILE_EXIT:
            DestroyWindow(hwnd);
            break;
        case IDM_HELP_KEYBOARD:
            g_state.help_visible = !g_state.help_visible;
            InvalidateRect(hwnd, NULL, FALSE);
            break;
        case IDM_HELP_ABOUT:
            MessageBoxW(hwnd,
                L"QsoRipper\r\n\r\n"
                L"Keyboard-first ham radio logging.\r\n\r\n"
                L"Press F1 for keyboard shortcuts.",
                L"About QsoRipper", MB_OK | MB_ICONINFORMATION);
            break;
        }
        break;

    case WM_DESTROY:
        KillTimer(hwnd, TIMER_ID);
        if (g_state.hFont) DeleteObject(g_state.hFont);
        if (g_state.hFontBold) DeleteObject(g_state.hFontBold);
        if (g_state.hFontSmall) DeleteObject(g_state.hFontSmall);
        if (g_state.hFontSmallBold) DeleteObject(g_state.hFontSmallBold);
        free(g_state.recent_qsos);
        PostQuitMessage(0);
        break;

    case WM_ERASEBKGND:
        return 1; /* Prevent flicker — we paint everything */

    case WM_PAINT:
    {
        PAINTSTRUCT ps;
        HDC hdc = BeginPaint(hwnd, &ps);
        RECT rc;
        GetClientRect(hwnd, &rc);
        PaintAll(hwnd, hdc, &rc);
        EndPaint(hwnd, &ps);
        break;
    }

    case WM_SIZE:
        InvalidateRect(hwnd, NULL, FALSE);
        break;

    case WM_TIMER:
        if (wParam == TIMER_ID)
            OnTimer(hwnd);
        break;

    case WM_KEYDOWN:
        OnKeyDown(hwnd, wParam, lParam);
        break;

    case WM_CHAR:
        OnChar(hwnd, wParam);
        break;

    case WM_SYSKEYDOWN:
        /* VK_MENU (Alt key itself), Space, F4, F, H must reach DefWindowProc
           so the menu bar, Alt+F4 close, and system menu work correctly.
           Kill the timer first so the 100ms tick and any pending lookup
           subprocess cannot block DefWindowProc's modal menu loop.
           NOTE: break exits to return 0 — must use explicit DefWindowProcW. */
        if (wParam == VK_MENU || wParam == VK_SPACE || wParam == VK_F4 ||
            wParam == 'F'     || wParam == 'H') {
            KillTimer(hwnd, TIMER_ID);
            return DefWindowProcW(hwnd, msg, wParam, lParam);
        }
        /* Handle our custom Alt+key field-navigation shortcuts */
        OnKeyDown(hwnd, wParam, lParam);
        return 0;

    case WM_SYSCHAR:
        /* Let menu accelerator chars and system chars reach DefWindowProc */
        if (wParam == 'f' || wParam == 'F' || wParam == 'h' || wParam == 'H' ||
            wParam == ' ') {
            return DefWindowProcW(hwnd, msg, wParam, lParam);
        }
        return 0;

    case WM_MENUCHAR:
        /* Suppress the error beep when an Alt+key has no matching menu item */
        return MAKELRESULT(0, MNC_CLOSE);

    case WM_EXITMENULOOP:
        /* Restart the timer that was suspended in WM_SYSKEYDOWN */
        SetTimer(hwnd, TIMER_ID, TIMER_MS, NULL);
        break;

    case WM_APP_LOOKUP_DONE:
    {
        LookupResultMsg *res = (LookupResultMsg *)lParam;
        if (res) {
            if (_stricmp(res->callsign, g_state.callsign) == 0 && res->has_data) {
                safe_strcpy(g_state.lookup_name, sizeof(g_state.lookup_name), res->name);
                safe_strcpy(g_state.lookup_qth,  sizeof(g_state.lookup_qth),  res->qth);
                safe_strcpy(g_state.lookup_grid, sizeof(g_state.lookup_grid), res->grid);
                safe_strcpy(g_state.lookup_country, sizeof(g_state.lookup_country), res->country);
                g_state.lookup_cq_zone = res->cq_zone;
                g_state.has_lookup = 1;
                if (g_state.worked_name[0] == 0)
                    safe_strcpy(g_state.worked_name, sizeof(g_state.worked_name), res->name);
                if (g_state.qth[0] == 0)
                    safe_strcpy(g_state.qth, sizeof(g_state.qth), res->qth);
                safe_strcpy(g_state.last_looked_up, sizeof(g_state.last_looked_up), res->callsign);
            }
            g_state.lookup_in_progress = 0;
            free(res);
        }
        InvalidateRect(hwnd, NULL, FALSE);
        break;
    }

    case WM_APP_RIG_DONE:
    {
        RigPollResult *res = (RigPollResult *)lParam;
        g_state.rig_poll_in_progress = 0;
        if (res) {
            if (g_state.rig_enabled) {
                g_state.rig_connected = res->connected;
                if (res->connected) {
                    safe_strcpy(g_state.rig_freq_display, sizeof(g_state.rig_freq_display), res->freq_display);
                    safe_strcpy(g_state.rig_freq_mhz, sizeof(g_state.rig_freq_mhz), res->freq_mhz);
                    safe_strcpy(g_state.rig_band, sizeof(g_state.rig_band), res->band);
                    safe_strcpy(g_state.rig_mode, sizeof(g_state.rig_mode), res->mode);
                    /* Auto-fill band/mode/freq when callsign is empty */
                    if (g_state.callsign[0] == 0) {
                        for (int bi = 0; bi < NUM_BANDS; bi++) {
                            if (_stricmp(BANDS[bi], res->band) == 0) {
                                g_state.band_idx = bi;
                                break;
                            }
                        }
                        for (int mi = 0; mi < NUM_MODES; mi++) {
                            if (_stricmp(MODES[mi], res->mode) == 0) {
                                g_state.mode_idx = mi;
                                ApplyModeDefaults();
                                break;
                            }
                        }
                        if (res->freq_mhz[0])
                            safe_strcpy(g_state.freq_mhz, sizeof(g_state.freq_mhz), res->freq_mhz);
                    }
                }
            }
            free(res);
        }
        InvalidateRect(hwnd, NULL, FALSE);
        break;
    }

    case WM_APP_QSO_LOADED:
    {
        QsoLoadResult *res = (QsoLoadResult *)lParam;
        g_state.qso_loading = 0;
        if (res) {
            free(g_state.recent_qsos);
            g_state.recent_qsos = res->qsos;
            g_state.recent_count = res->count;
            g_state.recent_capacity = res->capacity;
            free(res);
        }
        InvalidateRect(hwnd, NULL, FALSE);
        break;
    }

    case WM_MOUSEWHEEL:
        if (g_state.qso_list_focused) {
            int delta = GET_WHEEL_DELTA_WPARAM(wParam);
            if (delta > 0 && g_state.qso_selected > 0)
                g_state.qso_selected--;
            else if (delta < 0 && g_state.qso_selected < g_state.recent_count - 1)
                g_state.qso_selected++;
            InvalidateRect(hwnd, NULL, FALSE);
        }
        break;

    case WM_LBUTTONDOWN:
    {
        int mx = GET_X_LPARAM(lParam);
        int my = GET_Y_LPARAM(lParam);
        int cw = g_state.char_w;
        int ch = g_state.char_h;
        if (cw == 0 || ch == 0) break;

        RECT rc;
        GetClientRect(hwnd, &rc);
        (void)rc; /* layout uses cw/ch relative math */

        /* Compute layout positions matching PaintAll */
        int header_h = ch * 3 + 4;
        int status_h = ch + 4;
        int row_h = ch + 8;
        int form_start = header_h + status_h;
        int form_h = g_state.advanced_view
            ? (row_h * 12 + ch + 10)
            : (row_h * 9 + ch + 10);
        int form_end = form_start + form_h;
        int lookup_h = ch * 5 + 8;
        int qso_list_start = form_end + lookup_h;

        /* Click in form area? */
        if (my >= form_start && my < form_end && !g_state.advanced_view) {
            int pad = cw * 2;
            int label_w = cw * 10;
            int fy = form_start + ch + 4; /* first row y */
            int row = (my - fy) / row_h;
            int clicked = -1;

            if (row == 0) {
                /* Callsign / Band / Mode */
                int cx = pad + label_w;
                int bx = cx + 14 * cw + 16 + 5 * cw;
                int modex = bx + 8 * cw + 16 + 5 * cw;
                if (mx >= cx && mx < cx + 14 * cw + 6) clicked = FIELD_CALLSIGN;
                else if (mx >= bx && mx < bx + 8 * cw + 6) clicked = FIELD_BAND;
                else if (mx >= modex && mx < modex + 8 * cw + 6) clicked = FIELD_MODE;
            } else if (row == 1) {
                /* RST Sent / RST Rcvd */
                int cx = pad + label_w;
                int rx = cx + 6 * cw + 16 + 9 * cw;
                if (mx >= cx && mx < cx + 6 * cw + 6) clicked = FIELD_RST_SENT;
                else if (mx >= rx && mx < rx + 6 * cw + 6) clicked = FIELD_RST_RCVD;
            } else if (row == 2) {
                clicked = FIELD_COMMENT;
            } else if (row == 3) {
                clicked = FIELD_NOTES;
            } else if (row == 4) {
                /* Freq / Date / Time */
                int cx = pad + label_w;
                int dx = cx + 10 * cw + 16 + 5 * cw;
                int tx = dx + 12 * cw + 16 + 5 * cw;
                if (mx >= cx && mx < cx + 10 * cw + 6) clicked = FIELD_FREQ;
                else if (mx >= dx && mx < dx + 12 * cw + 6) clicked = FIELD_DATE;
                else if (mx >= tx && mx < tx + 6 * cw + 6) clicked = FIELD_TIME;
            }

            if (clicked >= 0) {
                g_state.focused_field = (enum Field)clicked;
                g_state.qso_list_focused = 0;
                g_state.search_focused = 0;
                /* Place cursor at click position within the field */
                char *buf = FieldBuffer((enum Field)clicked);
                if (buf) {
                    int field_x;
                    if (clicked == FIELD_CALLSIGN) field_x = pad + label_w;
                    else if (clicked == FIELD_COMMENT || clicked == FIELD_NOTES) field_x = pad + label_w;
                    else if (clicked == FIELD_RST_SENT) field_x = pad + label_w;
                    else if (clicked == FIELD_FREQ) field_x = pad + label_w;
                    else field_x = 0;
                    int pos = (mx - field_x - 3) / cw;
                    int len = (int)strlen(buf);
                    if (pos < 0) pos = 0;
                    if (pos > len) pos = len;
                    g_state.cursor_pos[clicked] = pos;
                }
                InvalidateRect(hwnd, NULL, FALSE);
            }
        }
        /* Click in QSO list area? */
        else if (my >= qso_list_start && g_state.recent_count > 0) {
            int lrh = g_state.qso_list_row_h > 0 ? g_state.qso_list_row_h : (ch + 4);
            int header_row_h = lrh + 2;
            int list_content_start = qso_list_start + ch + 4 + header_row_h;
            if (my >= list_content_start) {
                int row_idx = (my - list_content_start) / lrh + g_state.qso_scroll;
                if (row_idx >= 0 && row_idx < g_state.recent_count) {
                    g_state.qso_list_focused = 1;
                    g_state.search_focused = 0;
                    g_state.qso_selected = row_idx;
                    InvalidateRect(hwnd, NULL, FALSE);
                }
            } else {
                /* Clicked in QSO list header area — just focus the list */
                g_state.qso_list_focused = 1;
                g_state.search_focused = 0;
                if (g_state.qso_selected < 0 && g_state.recent_count > 0)
                    g_state.qso_selected = 0;
                InvalidateRect(hwnd, NULL, FALSE);
            }
        }
        break;
    }

    case WM_LBUTTONDBLCLK:
    {
        int mx = GET_X_LPARAM(lParam);
        int my = GET_Y_LPARAM(lParam);
        int cw = g_state.char_w;
        int ch = g_state.char_h;
        if (cw == 0 || ch == 0) break;
        (void)mx;

        int header_h = ch * 3 + 4;
        int status_h = ch + 4;
        int row_h = ch + 8;
        int form_start = header_h + status_h;
        int form_h = g_state.advanced_view
            ? (row_h * 12 + ch + 10)
            : (row_h * 9 + ch + 10);
        int form_end = form_start + form_h;
        int lookup_h = ch * 5 + 8;
        int qso_list_start = form_end + lookup_h;

        if (my >= qso_list_start && g_state.recent_count > 0) {
            int lrh = g_state.qso_list_row_h > 0 ? g_state.qso_list_row_h : (ch + 4);
            int header_row_h = lrh + 2;
            int list_content_start = qso_list_start + ch + 4 + header_row_h;
            if (my >= list_content_start) {
                int row_idx = (my - list_content_start) / lrh + g_state.qso_scroll;
                if (row_idx >= 0 && row_idx < g_state.recent_count) {
                    g_state.qso_selected = row_idx;
                    LoadSelectedQso();
                    InvalidateRect(hwnd, NULL, FALSE);
                }
            }
        }
        break;
    }

    case WM_GETMINMAXINFO:
    {
        MINMAXINFO *mmi = (MINMAXINFO *)lParam;
        mmi->ptMinTrackSize.x = 800;
        mmi->ptMinTrackSize.y = 600;
        break;
    }

    default:
        return DefWindowProcW(hwnd, msg, wParam, lParam);
    }
    return 0;
}

/* ── Entry point ───────────────────────────────────────────────────────── */

_Use_decl_annotations_
int WINAPI wWinMain(HINSTANCE hInstance, HINSTANCE hPrevInstance,
                    LPWSTR lpCmdLine, int nCmdShow)
{
    (void)hPrevInstance;
    (void)lpCmdLine;

    /* Initialize common controls (for visual styles if needed) */
    INITCOMMONCONTROLSEX icc = { sizeof(icc), ICC_STANDARD_CLASSES };
    InitCommonControlsEx(&icc);

    /* Initialize app state */
    InitState();
    FindCliPath();

    /* Register window class */
    WNDCLASSEXW wc = {0};
    wc.cbSize        = sizeof(wc);
    wc.style         = CS_HREDRAW | CS_VREDRAW | CS_DBLCLKS;
    wc.lpfnWndProc   = WndProc;
    wc.hInstance      = hInstance;
    wc.hCursor       = LoadCursor(NULL, IDC_ARROW);
    wc.hbrBackground = NULL; /* We handle all painting */
    wc.lpszClassName = WINDOW_CLASS;
    wc.hIcon         = LoadIcon(NULL, IDI_APPLICATION);
    wc.hIconSm       = LoadIcon(NULL, IDI_APPLICATION);

    if (!RegisterClassExW(&wc)) {
        MessageBoxW(NULL, L"Failed to register window class",
                    APP_TITLE, MB_ICONERROR);
        return 1;
    }

    /* Calculate initial window size: 100 cols x 45 rows equivalent */
    int init_w = 1100;
    int init_h = 780;

    HWND hwnd = CreateWindowExW(
        0, WINDOW_CLASS, APP_TITLE,
        WS_OVERLAPPEDWINDOW | WS_CLIPCHILDREN,
        CW_USEDEFAULT, CW_USEDEFAULT, init_w, init_h,
        NULL, NULL, hInstance, NULL);

    if (!hwnd) {
        MessageBoxW(NULL, L"Failed to create window",
                    APP_TITLE, MB_ICONERROR);
        return 1;
    }

    ShowWindow(hwnd, nCmdShow);
    UpdateWindow(hwnd);

    /* Message loop */
    MSG msg;
    while (GetMessageW(&msg, NULL, 0, 0)) {
        TranslateMessage(&msg);
        DispatchMessageW(&msg);
    }

    return (int)msg.wParam;
}
