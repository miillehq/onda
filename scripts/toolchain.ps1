$ErrorActionPreference = 'Stop'
if (-not (Get-Command cmake -ErrorAction SilentlyContinue)) {
    $visualStudioLocator = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio/Installer/vswhere.exe'
    if (Test-Path -LiteralPath $visualStudioLocator) {
        $visualStudioDirectory = & $visualStudioLocator -latest -products '*' -property installationPath
        $cmakeDirectory = Join-Path $visualStudioDirectory 'Common7/IDE/CommonExtensions/Microsoft/CMake/CMake/bin'
        if (Test-Path -LiteralPath (Join-Path $cmakeDirectory 'cmake.exe')) { $env:PATH = "$cmakeDirectory;$env:PATH" }
    }
}
if (-not $env:LIBCLANG_PATH) {
    $clangCandidates = @((Join-Path $env:ProgramFiles 'LLVM/bin'), (Join-Path $env:LOCALAPPDATA 'OndaBuildTools/libclang/clang/native'))
    foreach ($clangCandidate in $clangCandidates) { if (Test-Path -LiteralPath (Join-Path $clangCandidate 'libclang.dll')) { $env:LIBCLANG_PATH = $clangCandidate; break } }
}
if (-not (Get-Command cmake -ErrorAction SilentlyContinue)) { throw 'CMake is required. See docs/release.md.' }
if (-not $env:LIBCLANG_PATH -or -not (Test-Path -LiteralPath (Join-Path $env:LIBCLANG_PATH 'libclang.dll'))) { throw 'LLVM libclang.dll is required. Set LIBCLANG_PATH; see docs/release.md.' }
