param([Parameter(Mandatory)][string]$Executable,[Parameter(Mandatory)][int]$OwnerProcessId)
$ErrorActionPreference = 'Stop'
if ($env:RUNNER_ENVIRONMENT -ne 'github-hosted' -or $env:RUNNER_OS -ne 'Windows') { throw 'Disposable hosted Windows is required.' }
Add-Type -AssemblyName System.Windows.Forms
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class FirstActionWindow {
    [DllImport("user32.dll", CharSet=CharSet.Unicode)]
    public static extern IntPtr FindWindow(IntPtr className, string title);
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
    $pill = [FirstActionWindow]::FindWindow([IntPtr]::Zero, 'Onda dictation')
    [uint32]$owner = 0
    $null = [FirstActionWindow]::GetWindowThreadProcessId($pill, [ref]$owner)
    if ($pill -eq [IntPtr]::Zero) {throw 'Could not find the installed dictation indicator by title.'}
    if ($owner -ne $OwnerProcessId) {throw 'A different process owns the indicator.'}
    if ([FirstActionWindow]::IsWindowVisible($pill)) {throw 'The installed indicator was already visible.'}
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
    $registration = 'HKLM:/SOFTWARE/WOW6432Node/Microsoft/EdgeUpdate/ClientState/{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}'
    $runtime = (Get-ItemProperty -LiteralPath $registration -ErrorAction Stop).EBWebView
    return [pscustomobject]@{ initialPillHidden=$true; existingPillShown=$true; firstVisibleMs=$firstVisible; focusPreserved=$true; secondaryExit=$toggle.ExitCode; systemImage=$env:ImageOS; runtimeVersion=(Split-Path -Leaf $runtime); standaloneAngle=(Test-Path -LiteralPath (Join-Path $runtime 'libGLESv2.dll')) }
} finally {
    if ($toggle) { $toggle.Dispose() }
    $form.Dispose()
}
