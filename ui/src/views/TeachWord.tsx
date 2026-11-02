import { Check, CircleCheck, Mic } from "lucide-preact";
import { useEffect, useRef, useState } from "preact/hooks";
import { ask, toAppError } from "../api";
import { Button } from "../components/controls";
import { LevelBars } from "../components/feedback";
import { Dialog } from "../components/layout";
import type { TeachTake } from "../generated/api";
import type { AppError, DictionaryEntry } from "../generated/core";
import { presentError } from "../lib/errors";
import { dictionary, micLevel, thisDevice, toast } from "../lib/store";

/** Long enough to say a name and stop; short enough that silence doesn't invite guesses. */
const TAKE_MS = 2500;
const SUGGESTED_TAKES = 3;
const MAX_TAKES = 6;

/** Mirrors the native lookup key: Unicode-normalized, lower case, single spaces. */
const keyOf = (text: string) => text.normalize("NFKC").toLocaleLowerCase().split(/\s+/).filter(Boolean).join(" ");

type Outcome = "right" | "covered" | "new";

function outcome(take: TeachTake, word: string): Outcome {
  if (keyOf(take.heard) === keyOf(word)) return "right";
  return keyOf(take.written).includes(keyOf(word)) ? "covered" : "new";
}

/**
 * Say a dictionary word a few times; whatever the local model writes instead becomes a
 * replacement. Speech models take spelling hints, not pronunciations, so this learns the
 * misspellings this voice and model actually produce.
 */
export function TeachWord({ entry, onClose }: { entry: DictionaryEntry; onClose: () => void }) {
  const word = entry.phrase;
  const [takes, setTakes] = useState<TeachTake[]>([]);
  const [stage, setStage] = useState<"ready" | "listening" | "writing">("ready");
  const [error, setError] = useState<AppError | null>(null);
  const [skipped, setSkipped] = useState<Set<string>>(new Set());
  const [saving, setSaving] = useState(false);
  const open = useRef(true);
  useEffect(
    () => () => {
      open.current = false;
    },
    [],
  );

  const variants = new Map<string, { heard: string; count: number; result: Outcome }>();
  for (const take of takes) {
    const key = keyOf(take.heard);
    const known = variants.get(key);
    if (known) known.count += 1;
    else variants.set(key, { heard: take.heard, count: 1, result: outcome(take, word) });
  }
  const fresh = [...variants.entries()].filter(([, variant]) => variant.result === "new");
  const chosen = fresh.filter(([key]) => !skipped.has(key));

  const record = async () => {
    setError(null);
    setStage("listening");
    // Recording and transcription are one native call; the label follows the recording time.
    const writing = window.setTimeout(() => open.current && setStage("writing"), TAKE_MS);
    try {
      const take = await ask({ command: "teachWord", args: { durationMs: TAKE_MS } }, "teachTake");
      if (open.current) setTakes((list) => [...list, take]);
    } catch (failure) {
      if (open.current) setError(toAppError(failure));
    } finally {
      window.clearTimeout(writing);
      if (open.current) setStage("ready");
    }
  };

  const save = async () => {
    setSaving(true);
    let saved = 0;
    try {
      for (const [key, variant] of chosen) {
        const existing = dictionary.value.find((item) => keyOf(item.phrase) === key);
        // A phrase that is itself a dictionary word is left alone rather than rewritten.
        if (existing?.kind === "vocabulary") continue;
        const replacement: DictionaryEntry = existing
          ? { ...existing, replacement: word, enabled: true }
          : { id: crypto.randomUUID(), kind: "replacement", phrase: variant.heard, replacement: word, enabled: true, starred: false, source: "manual" };
        dictionary.value = await ask({ command: "saveDictionaryEntry", args: { entry: replacement } }, "dictionary");
        saved += 1;
      }
      const heard = chosen.map(([, variant]) => `“${variant.heard}”`);
      toast({ tone: "ok", title: saved ? `Onda will write “${word}” when it hears ${heard.join(" or ")}` : "Nothing new to add" }, 6000);
      onClose();
    } catch (failure) {
      setError(toAppError(failure));
    } finally {
      setSaving(false);
    }
  };

  const busy = stage !== "ready";
  const allRight = takes.length > 0 && fresh.length === 0;
  const failure = error ? presentError(error) : null;
  return (
    <Dialog
      open
      title={`Teach Onda how “${word}” sounds`}
      description={`Say just the word, the way you normally would. Onda listens on ${thisDevice.value}, and anything it writes differently becomes a replacement.`}
      onClose={onClose}
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>
            {takes.length ? "Cancel" : "Close"}
          </Button>
          {allRight ? (
            <Button variant="primary" onClick={onClose}>
              Done
            </Button>
          ) : (
            <Button variant="primary" busy={saving} disabled={busy || !chosen.length} onClick={() => void save()}>
              {chosen.length === 0 ? "Add replacements" : chosen.length === 1 ? "Add 1 replacement" : `Add ${chosen.length} replacements`}
            </Button>
          )}
        </>
      }
    >
      <div class={`teach-stage is-${stage}`}>
        <p class="teach-word serif">{word}</p>
        <LevelBars level={micLevel} bars={20} active={stage === "listening"} />
        <p class="teach-status" role="status">
          {stage === "listening" ? `Listening… say “${word}”` : stage === "writing" ? "Writing out what Onda heard…" : takes.length ? `Take ${takes.length} done. ${takes.length < SUGGESTED_TAKES ? "One or two more helps." : "Add more takes or save."}` : `${SUGGESTED_TAKES} short takes work best.`}
        </p>
        <span class="teach-progress" aria-hidden="true">
          <i />
        </span>
        <Button variant="primary" icon={<Mic size={16} />} busy={busy} disabled={takes.length >= MAX_TAKES} onClick={() => void record()} data-autofocus>
          {takes.length ? "Say it again" : "Say it"}
        </Button>
      </div>

      {failure && error && (
        <p class="field-error" role="alert">
          {failure.title}. {error.message}
        </p>
      )}

      {variants.size > 0 && (
        <ul class="teach-takes" aria-label="What Onda heard">
          {[...variants.entries()].map(([key, variant]) => (
            <li key={key} class={`teach-take is-${variant.result}`}>
              <span class="teach-heard serif">
                “{variant.heard}”{variant.count > 1 && <span class="teach-count tabular"> ×{variant.count}</span>}
              </span>
              {variant.result === "new" ? (
                <label class="teach-choice">
                  <input
                    type="checkbox"
                    checked={!skipped.has(key)}
                    onChange={(event) => {
                      const next = new Set(skipped);
                      if ((event.currentTarget as HTMLInputElement).checked) next.delete(key);
                      else next.add(key);
                      setSkipped(next);
                    }}
                  />
                  Write “{word}” instead
                </label>
              ) : (
                <span class="teach-ok">
                  {variant.result === "right" ? <CircleCheck size={15} aria-hidden="true" /> : <Check size={15} aria-hidden="true" />}
                  {variant.result === "right" ? "Heard right" : "Already fixed by your dictionary"}
                </span>
              )}
            </li>
          ))}
        </ul>
      )}
      {allRight && <p class="footnote">Onda already gets “{word}” right with your voice. Nothing to add.</p>}
      {fresh.length > 0 && <p class="footnote">Replacements apply everywhere you dictate, so untick anything you also say on purpose.</p>}
    </Dialog>
  );
}
