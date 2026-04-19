#include <windows.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

void qsr_test_set_cli_path(const WCHAR *path);
char *qsr_test_run_qr_command(const char *args);

static int find_powershell_path(WCHAR *buffer, size_t buffer_len)
{
    DWORD n = GetEnvironmentVariableW(L"SystemRoot", buffer, (DWORD)buffer_len);
    if (n == 0 || n >= buffer_len) {
        return 0;
    }

    if (wcscat_s(buffer, buffer_len, L"\\System32\\WindowsPowerShell\\v1.0\\powershell.exe") != 0) {
        return 0;
    }

    return GetFileAttributesW(buffer) != INVALID_FILE_ATTRIBUTES;
}

int main(void)
{
    WCHAR powershell_path[MAX_PATH] = {0};
    if (!find_powershell_path(powershell_path, MAX_PATH)) {
        fprintf(stderr, "failed to locate powershell.exe\n");
        return 2;
    }

    qsr_test_set_cli_path(powershell_path);

    /* Regression for WIN32-BUG-4:
       non-zero exit with output must be treated as failure. */
    {
        char *out = qsr_test_run_qr_command(
            "-NoProfile -NonInteractive -Command \"Write-Output 'synthetic failure'; exit 17\"");
        if (out != NULL) {
            fprintf(stderr, "expected failure (NULL), got output: %s\n", out);
            free(out);
            return 1;
        }
    }

    /* Success path remains unchanged. */
    {
        char *out = qsr_test_run_qr_command(
            "-NoProfile -NonInteractive -Command \"Write-Output 'ok'; exit 0\"");
        if (out == NULL) {
            fprintf(stderr, "expected output for successful command\n");
            return 1;
        }
        if (strstr(out, "ok") == NULL) {
            fprintf(stderr, "expected successful output payload, got: %s\n", out);
            free(out);
            return 1;
        }
        free(out);
    }

    return 0;
}
