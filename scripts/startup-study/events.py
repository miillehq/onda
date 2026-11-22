"""Keep event identifiers, timings and process roles from a disposable first launch."""
from collections import Counter
from datetime import datetime
import json
from pathlib import Path, PureWindowsPath
import re
import sys
import xml.etree.ElementTree as ET

source, output = map(Path, sys.argv[1:])
reports = [json.loads(f.read_text(encoding="utf-8-sig")) for f in Path(".artifacts/installed-smoke").glob("*.json")]
launch = datetime.fromisoformat(reports[0]["launchUtc"]).timestamp() if reports else 0
events, counts, layouts = [], Counter(), {}
ns = "{http://schemas.microsoft.com/win/2004/08/events/event}"
def child(element, key):
    return element.find(ns+key) if element is not None else None
def label(value):
    return value[:160] if value and re.fullmatch(r"[A-Za-z0-9_:. /()<>+-]{1,200}",value) and not any(x in value for x in ("http","@","?","--","/Users/")) else ""
for _, event in ET.iterparse(source, events=("end",)):
    if event.tag.rpartition("}")[2] != "Event": continue
    system = child(event,"System")
    provider, at, run = (child(system,k) for k in ("Provider","TimeCreated","Execution"))
    if provider is None or at is None or run is None: event.clear();continue
    ms=round((datetime.fromisoformat(at.get("SystemTime")).timestamp()-launch)*1000,1)
    if ms < -500: event.clear();continue
    provider_name=provider.get("Name") or provider.get("Guid") or ""
    render=child(event,"RenderingInfo"); task=child(render,"Task"); opcode=child(render,"Opcode")
    payload=child(event,"EventData")
    fields={n.get("Name",""):n.text or "" for n in list(payload) if n.get("Name")} if payload is not None else {}
    name=(payload.get("Name") if payload is not None else "") or (task.text if task is not None else "") or (child(system,"EventID").text if child(system,"EventID") is not None else "")
    k=str(provider_name)+":"+str(name); counts[k]+=1
    if k not in layouts: layouts[k]=list(fields)
    pid=run.get("ProcessID"); tid=run.get("ThreadID")
    if "Kernel-Process" in provider_name or "22fb2cd6" in provider_name.lower():
        image=PureWindowsPath(fields.get("ImageName","")).name.lower()
        if image in ("onda.exe","onda-input.exe","msedgewebview2.exe","msedge.exe","msedgeupdate.exe","msmpeng.exe"):
            role=re.search(r"--type=([A-Za-z0-9_.-]+)",fields.get("CommandLine",""))
            events.append([ms,pid,tid,"process",name,fields.get("ProcessID"),fields.get("ParentProcessID"),image,role[1] if role else "host/browser"])
    elif "edge" in provider_name.lower() or any(x in provider_name.lower() for x in ("e16ec3d2","3a5f2396")):
        details={key:label(v) for key,v in fields.items() if key.lower() in ("name","eventname","task","category","function","functionname","result","hresult","status","duration","component","phase","classname","method","methodname") and label(v)}
        if len(events)<60000:events.append([ms,pid,tid,"edge",label(name),opcode.text if opcode is not None else "",details])
    elif "capi" in provider_name.lower() and len(events)<60000:
        events.append([ms,pid,tid,"capi",name])
    event.clear()
(output/"events.json").write_text(json.dumps({"types":counts,"fieldNames":layouts,"events":events},separators=(",",":")),encoding="utf-8")
print("Events decoded:",sum(counts.values()),"selected:",len(events))
