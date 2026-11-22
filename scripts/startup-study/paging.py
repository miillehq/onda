"""Cross-reference native hard page faults to the measured process tree; retain no user paths."""
import collections, datetime, json, re, sys
from pathlib import Path, PureWindowsPath
import xml.etree.ElementTree as ET
src, output=map(Path,sys.argv[1:])
reports=[json.loads(f.read_text(encoding='utf-8-sig')) for f in Path('.artifacts/installed-smoke').glob('*.json')]
launch=datetime.datetime.fromisoformat(reports[0]['launchUtc']).timestamp()
edge=json.loads((output/'events.json').read_text())
owned={int(str(e[5]).strip()) for e in edge['events'] if e[3]=='process' and e[4]=='ProcessStart' and e[7] in ('onda.exe','onda-input.exe','msedgewebview2.exe')}
tasks=collections.Counter(); layouts={}; filenames={}; threads={}; faults=[]; reads=[]
ns='{http://schemas.microsoft.com/win/2004/08/events/event}'
def get(node,name):
    return node.find(ns+name) if node is not None else None
def value(node,name):
    sub=get(node,name);return (sub.text or '').strip() if sub is not None else ''
def number(text):
    try: return int(str(text).strip(),0)
    except (ValueError,TypeError): return None
for _,ev in ET.iterparse(src,events=('end',)):
    if ev.tag.rpartition('}')[2]!='Event':continue
    system=get(ev,'System'); ex=get(system,'Execution'); stamp=get(system,'TimeCreated'); provider=get(system,'Provider')
    if ex is None or stamp is None or provider is None:ev.clear();continue
    render=get(ev,'RenderingInfo'); data=get(ev,'EventData')
    fields={n.get('Name'):n.text or '' for n in list(data) if n.get('Name')} if data is not None else {}
    prov=(provider.get('Name') or provider.get('Guid') or '').strip()
    op=value(render,'Opcode'); task=value(render,'Task'); name=(data.get('Name') if data is not None else '') or op or task or value(system,'EventID')
    key=prov+' / '+name
    tasks[key]+=1
    if key not in layouts:layouts[key]=list(fields)
    fields={k.lower():v.strip() for k,v in fields.items()}
    file=fields.get('filename') or fields.get('openpath')
    obj=fields.get('fileobject') or fields.get('filekey')
    if obj and file:
        base=PureWindowsPath(file).name.lower()
        known=base in ('msedge.dll','msedgewebview2.exe','msedge_elf.dll','icudtl.dat','resources.pak','v8_context_snapshot.bin','onda.exe','onda-input.exe','embeddedbrowserwebview.dll') or 'edgewebview' in file.lower()
        filenames[obj]=base if known and re.fullmatch(r'[a-z0-9._-]{1,80}',base) else 'other'
    tid=number(fields.get('tthreadid',fields.get('threadid')));pid=number(fields.get('processid'))
    if tid is not None and pid is not None and pid in owned:threads[tid]=pid
    ms=round((datetime.datetime.fromisoformat(stamp.get('SystemTime')).timestamp()-launch)*1000,2)
    if 'initialtime' in fields and 'bytecount' in fields and ms>0:
        faults.append((ms,number(ex.get('ProcessID')),tid,fields.get('initialtime'),number(fields['bytecount']),obj,stamp.get('SystemTime')))
    ev.clear()
selected=[[ms,threads.get(tid,pid),tid,initial,count,filenames.get(obj,'unknown'),at] for ms,pid,tid,initial,count,obj,at in faults if pid in owned or tid in threads]
(output/'paging.json').write_text(json.dumps({'owned':list(owned),'faults':selected,'types':tasks,'layouts':layouts,'totalFaults':len(faults)},separators=(',',':')),encoding='utf-8')
print('Relevant hard faults',len(selected),'total',len(faults))
