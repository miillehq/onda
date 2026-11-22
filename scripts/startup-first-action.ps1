param([Parameter(Mandatory)][string]$Executable,[Parameter(Mandatory)][int]$OwnerProcessId)
$ErrorActionPreference = 'Stop'
if ($env:RUNNER_ENVIRONMENT -ne 'github-hosted' -or $env:RUNNER_OS -ne 'Windows') { throw 'Disposable hosted Windows is required.' }
Add-Type -AssemblyName System.Windows.Forms
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class FirstActionWindow {
    [DllImport("user32.dll", CharSet=CharSet.Unicode)]
    public static extern IntPtr FindWindow(string className, string title);
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr window);
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr window, out uint pid);
}
'@
$form = [Windows.Forms.Form]::new()
$toggle = $null
try {
    $form.Text = 'Onda synthetic focus fixture'
    $field = [Windows.Forms.TextBox]::new()
    $form.Controls.Add($field)
    $form.Show()
    $form.Activate()
    $null = $field.Focus()
    [Windows.Forms.Application]::DoEvents()
    $fixture = $form.Handle
    if ([FirstActionWindow]::GetForegroundWindow() -ne $fixture -or -not $field.Focused) { throw 'No foreground synthetic edit field.' }
    $pill = [FirstActionWindow]::FindWindow($null, 'Onda dictation')
    [uint32]$owner = 0
    $null = [FirstActionWindow]::GetWindowThreadProcessId($pill, [ref]$owner)
    if ($pill -eq [IntPtr]::Zero -or $owner -ne $OwnerProcessId -or [FirstActionWindow]::IsWindowVisible($pill)) { throw 'The first action requires the installed shell to own a hidden indicator.' }
    $clock = [Diagnostics.Stopwatch]::StartNew()
    $toggle = Start-Process -FilePath $Executable -ArgumentList '--dictate' -WindowStyle Hidden -PassThru
    do {
        [Windows.Forms.Application]::DoEvents()
        if ([FirstActionWindow]::GetForegroundWindow() -ne $fixture -or -not $field.Focused) {throw 'The first dictation action took focus from the synthetic field.'}
        if ([FirstActionWindow]::IsWindowVisible($pill)) { break }
        Start-Sleep -Milliseconds 15
    } while ($clock.ElapsedMilliseconds -lt 5000)
    $firstVisible = $clock.ElapsedMilliseconds
    if (-not [FirstActionWindow]::IsWindowVisible($pill)) {throw 'The original installed indicator did not appear for the first action.'}
    Start-Sleep -Milliseconds 200
    [Windows.Forms.Application]::DoEvents()
    if ([FirstActionWindow]::GetForegroundWindow() -ne $fixture -or -not $field.Focused) {throw 'The indicator took focus from the synthetic field.'}
    if (-not $toggle.WaitForExit(5000) -or $toggle.ExitCode -ne 0) {throw 'The second instance failed to deliver its action.'}
    return [pscustomobject]@{ initialPillHidden=$true; existingPillShown=$true; firstVisibleMs=$firstVisible; focusPreserved=$true; secondaryExit=$toggle.ExitCode }
} finally {
    if ($toggle) { $toggle.Dispose() }
    $form.Dispose()
}
