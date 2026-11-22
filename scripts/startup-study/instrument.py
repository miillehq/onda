"""Add call boundaries to the locked Wry source in a disposable runner-only checkout."""
from pathlib import Path
import json
import os
import shutil
import subprocess
import sys

def cargo_metadata(*args):
    return json.loads(subprocess.check_output(["cargo","metadata",*args,"--format-version","1","--filter-platform","x86_64-pc-windows-msvc"],encoding="utf-8"))
meta=cargo_metadata('--locked')
temp=Path(os.environ['RUNNER_TEMP']).resolve()
overrides={}
for name,version in (("wry","0.57.0"),("webview2-com","0.39.1")):
    item,=[p for p in meta['packages'] if p['name']==name and p['version']==version]
    path=temp/('onda-measure-'+name)
    shutil.copytree(Path(item['manifest_path']).parent,path)
    overrides[name]=path
file=overrides["wry"]/"src/webview2/mod.rs"
s=file.read_text(encoding="utf-8")
def before(anchor,label):
    global s
    assert s.count(anchor)==1,anchor
    indent=anchor[:len(anchor)-len(anchor.lstrip())]
    s=s.replace(anchor,indent+'onda_mark("'+label+'");\n'+anchor)
def after(anchor,label):
    global s
    assert s.count(anchor)==1,anchor
    indent=anchor[:len(anchor)-len(anchor.lstrip())]
    s=s.replace(anchor,anchor+'\n'+indent+'onda_mark("'+label+'");')
helper=r'''
fn onda_mark(name: &str) {
    use std::io::Write as _;
    if let Some(path) = std::env::var_os("ONDA_TRACE_PHASES") {
        if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
            let micros = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_micros();
            let _ = writeln!(file, "{{\"us\":{},\"pid\":{},\"stage\":\"{}\"}}", micros, std::process::id(), name);
        }
    }
}
'''
s=s.replace('mod drag_drop;',helper+'\nmod drag_drop;')
before('    let hwnd = Self::create_container_hwnd(parent, &attributes, is_child)?;', 'container.begin')
before('    let env = if let Some(env) = &pl_attrs.environment {','environment.begin')
before('    let controller = Self::create_controller(', 'environment.end')
before('    let webview = Self::init_webview(', 'controller.end')
before('    let drag_drop_controller = drop_handler.map(|handler| {','init_webview.end')
before('    Ok(w)\n', 'wry.ready')
before('    // Webview Settings','settings.begin')
before('    // Webview handlers','handlers.begin')
before('    // IPC handler','ipc.begin')
before('    // Custom protocols handler','protocols.begin')
before('    // Initialize main and subframe scripts','scripts.begin')
before('    // Enable clipboard','scripts.end')
before('      let js = HSTRING::from(js);\n        webview\n          .AddScriptToExecuteOnDocumentCreated','script.call')
a='      Box::new(|e, _| e),\n    )\n    .map_err(Into::into)\n  }\n\n  #[inline]\n  fn execute_script'
assert s.count(a)==1
s=s.replace(a,a.replace('Box::new(|e, _| e)','Box::new(|e, _| { onda_mark("script.callback"); e })'))
before('    if let Some(mut url) = attributes.url {','navigate.begin')
before('    // Subclass parent for resizing and focus','navigate.end')
file.write_text(s,encoding="utf-8",newline="\n")
file=overrides["webview2-com"]/'src/lib.rs'
s=file.read_text(encoding="utf-8")
old='''    loop {
        if let Ok(result) = rx.try_recv() {
            return Ok(result);
        }
'''
new='''    let mut reported = false;
    loop {
        match rx.try_recv() {
            Ok(result) => return Ok(result),
            Err(std::sync::mpsc::TryRecvError::Disconnected) if !reported => {
                if let Some(path) = std::env::var_os("ONDA_TRACE_PHASES") {
                    use std::io::Write as _;
                    if let Ok(mut file) = std::fs::OpenOptions::new().append(true).create(true).open(path) {
                        let micros = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_micros();
                        let _ = writeln!(file, "{{\\\"us\\\":{},\\\"stage\\\":\\\"completion.disconnected\\\"}}", micros);
                    }
                }
                reported=true;
            }
            Err(_) => {}
        }
'''
assert s.count(old)==1
file.write_text(s.replace(old,new),encoding="utf-8",newline="\n")
if sys.argv[1:] == ["--validate"]:
    print("All locked dependency instrumentation boundaries matched.")
    sys.exit(0)
with Path('.cargo/config.toml').open('a',encoding='utf-8',newline='\n') as configuration:
    configuration.write('\n[patch.crates-io]\n'+''.join(f'{n} = {{ path = "{p.as_posix()}" }}\n' for n,p in overrides.items()))
new_meta=cargo_metadata('--offline')
for name,path in overrides.items():
    assert any(p['name']==name and Path(p['manifest_path']).parent.resolve()==path for p in new_meta['packages'])
