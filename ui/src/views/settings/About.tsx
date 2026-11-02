import { Download, Gauge, RefreshCw, RotateCcw } from "lucide-preact";
import { useEffect, useState } from "preact/hooks";
import iconUrl from "../../assets/onda-icon-dark.svg";
import { ask } from "../../api";
import { Button, Keys } from "../../components/controls";
import { Card, Page, Row, Rows, Section } from "../../components/layout";
import type { ResourceUsage, UpdateInfo } from "../../generated/api";
import { formatBytes } from "../../lib/format";
import { navigate } from "../../lib/router";
import { cancelKeys, commandShortcutOnly, getResourceUsage, isWindows, settings, snapshot, toastError } from "../../lib/store";

const ROLE_NAMES: Record<string, string> = {
  app: "Onda",
  helper: "Shortcut helper",
  speech: "Speech recognition",
  webview: "Interface (web view)",
  other: "Other helpers",
};

const MEASURES: Record<string, string> = {
  privateWorkingSet: "Memory is the private working set, the same measure as Task Manager's Memory column.",
  privateCommit: "This Windows version doesn't report the private working set, so memory is private commit, which reads higher than Task Manager's Memory column.",
  privateResident: "Memory is private resident memory, close to what System Monitor shows.",
};

/** Onda's own process tree, measured by native code; Task Manager splits it into groups. */
function ResourceReport() {
  const [report, setReport] = useState<ResourceUsage | null>(null);
  const [measuring, setMeasuring] = useState(false);
  const [open, setOpen] = useState(false);
  const measure = async () => {
    setMeasuring(true);
    try {
      setReport(await getResourceUsage());
    } catch (error) {
      toastError(error, "Couldn't measure Onda's processes");
    } finally {
      setMeasuring(false);
    }
  };
  useEffect(() => {
    void measure();
  }, []);
  const groups = report
    ? Object.entries(
        report.processes.reduce<Record<string, { count: number; memory: number; cpu: number }>>((all, process) => {
          const group = (all[process.role] ??= { count: 0, memory: 0, cpu: 0 });
          group.count += 1;
          group.memory += process.memoryBytes;
          group.cpu += process.cpuPercent;
          return all;
        }, {}),
      )
    : [];
  const webviews = report?.processes.filter((process) => process.role === "webview").length ?? 0;
  return (
    <Section
      title="What Onda is using"
      description={
        isWindows.value
          ? "Includes the WebView2 processes that draw Onda's windows. Task Manager lists those separately, as “WebView2 Manager”."
          : "Onda draws its windows with WebKitGTK, whose processes (WebKitWebProcess, WebKitNetworkProcess) appear separately in System Monitor. This total includes them."
      }
    >
      <Card class="resources">
        <div class="resources-head">
          <div>
            <p class="tile-label">
              <Gauge size={16} aria-hidden="true" /> Onda in total
            </p>
            <p class="tile-value tabular">{report ? formatBytes(report.totalMemoryBytes) : "…"}</p>
            <p class="field-hint">{report ? `${report.totalCpuPercent.toFixed(1)}% CPU · ${report.processes.length} processes` : "Measuring…"}</p>
          </div>
          <Button size="sm" icon={<RefreshCw size={15} />} busy={measuring} onClick={() => void measure()}>
            Measure again
          </Button>
        </div>
        {report && (
          <table class="plain-table">
            <thead>
              <tr>
                <th scope="col">Part</th>
                <th scope="col" class="is-number">
                  Processes
                </th>
                <th scope="col" class="is-number">
                  Memory
                </th>
                <th scope="col" class="is-number">
                  CPU
                </th>
              </tr>
            </thead>
            <tbody>
              {groups.map(([role, group]) => (
                <tr key={role}>
                  <td>{ROLE_NAMES[role] ?? role}</td>
                  <td class="is-number">{group.count}</td>
                  <td class="is-number">{formatBytes(group.memory)}</td>
                  <td class="is-number">{group.cpu.toFixed(1)}%</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
        {report && (
          <button type="button" class="link-button" aria-expanded={open} onClick={() => setOpen(!open)}>
            {open ? "Hide processes" : "Show each process"}
          </button>
        )}
        {report && open && (
          <table class="plain-table">
            <tbody>
              {report.processes.map((process) => (
                <tr key={process.pid}>
                  <td class="mono">{process.name || "(unnamed)"}</td>
                  <td class="is-number mono">PID {process.pid}</td>
                  <td class="is-number">{formatBytes(process.memoryBytes)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
        {report && <p class="footnote">{MEASURES[report.memoryMeasure] ?? ""}</p>}
        {report && isWindows.value && webviews === 0 && (
          <p class="footnote">No WebView2 processes were found under Onda. That happens when Onda runs as administrator, where Windows starts WebView2 separately; they aren't counted here then.</p>
        )}
      </Card>
    </Section>
  );
}

export function About() {
  const value = settings.value;
  const info = snapshot.value;
  const [update, setUpdate] = useState<UpdateInfo | null>(null);
  const [checking, setChecking] = useState(false);
  const [installing, setInstalling] = useState(false);
  if (!value || !info) return null;
  const check = async () => {
    setChecking(true);
    try {
      setUpdate(await ask({ command: "checkUpdate" }, "update"));
    } catch (error) {
      toastError(error, "Couldn't check for updates");
    } finally {
      setChecking(false);
    }
  };
  const install = async () => {
    setInstalling(true);
    try {
      await ask({ command: "installUpdate" }, "unit");
    } catch (error) {
      setInstalling(false);
      toastError(error, "Update not installed");
    }
  };
  const cheats: { keys: number[]; what: string }[] = [
    { keys: value.pushToTalk.keys, what: "Hold to dictate" },
    ...(value.toggleEnabled ? [{ keys: value.toggleShortcut.keys, what: "Hands-free on and off" }] : []),
    { keys: value.transformShortcut.keys, what: "Rewrite selected text" },
    { keys: value.commandShortcut.keys, what: "Edit selected text with your voice" },
    { keys: cancelKeys(value.pushToTalk.keys), what: "Cancel" },
    { keys: [0xa2, 0x4b], what: "Search Onda (in the app)" },
  ];
  return (
    <Page title="Updates and help">
      <div class="about">
        <img src={iconUrl} alt="" width={84} height={84} />
        <div>
          <h2>{info.appName}</h2>
          <p class="muted">
            Version <span class="tabular">{info.version}</span> · by miille
          </p>
        </div>
      </div>

      <Section title="Updates">
        <Rows>
          <Row
            icon={<RefreshCw size={18} />}
            title={update ? (update.available ? `Version ${update.version} is available` : "You're up to date") : "Check for updates"}
            description={info.updaterConfigured ? "Updates are signed and verified before they install." : "This build doesn't include automatic updates."}
            control={
              update?.available ? (
                <Button variant="primary" icon={<Download size={16} />} busy={installing} onClick={() => void install()}>
                  Install and restart
                </Button>
              ) : (
                <Button size="sm" busy={checking} disabled={!info.updaterConfigured} onClick={() => void check()}>
                  Check now
                </Button>
              )
            }
          />
        </Rows>
      </Section>

      <Section title="Shortcuts at a glance">
        {!commandShortcutOnly.value ? (
          <ul class="cheats">
            {cheats.map((cheat) => (
              <li key={cheat.what}>
                <span>{cheat.what}</span>
                <Keys keys={cheat.keys} size="md" />
              </li>
            ))}
          </ul>
        ) : (
          <p class="muted">
            Bind a desktop shortcut to <span class="mono selectable">onda --dictate</span> to start and finish dictation, or use the tray menu. See Settings › Shortcuts.
          </p>
        )}
      </Section>

      <ResourceReport />

      <Section title="Help">
        <Rows>
          <Row
            icon={<RotateCcw size={18} />}
            title="Run setup again"
            description="Check your microphone, model and shortcut step by step."
            control={
              <Button size="sm" onClick={() => navigate("onboarding")}>
                Run setup
              </Button>
            }
          />
        </Rows>
        <p class="footnote">Onda is open source under the Apache 2.0 licence. Speech recognition uses whisper.cpp and OpenAI Whisper models (MIT).</p>
      </Section>
    </Page>
  );
}
