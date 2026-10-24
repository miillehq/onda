import { useEffect, useRef, useState } from "preact/hooks";
import type { AppError } from "../generated/core";
import { chordFrom, sameChord, virtualKey, VK } from "../lib/keys";
import { Button, Keys } from "./controls";

interface RecorderProps {
  value: readonly number[];
  label: string;
  onSave: (keys: number[]) => Promise<AppError | null>;
  disabled?: boolean;
}

/**
 * Click Change, press the new chord, release. Escape cancels. Native validation decides
 * conflicts and reserved keys; its message is shown inline under the keys.
 */
export function ShortcutRecorder({ value, label, onSave, disabled = false }: RecorderProps) {
  const [recording, setRecording] = useState(false);
  const [held, setHeld] = useState<number[]>([]);
  const [problem, setProblem] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const box = useRef<HTMLDivElement>(null);
  const candidate = useRef(new Set<number>());
  const down = useRef(new Set<number>());

  useEffect(() => {
    if (recording) box.current?.focus();
  }, [recording]);

  const stop = () => {
    setRecording(false);
    setHeld([]);
    candidate.current.clear();
    down.current.clear();
  };

  const commit = async () => {
    const { keys, problem: localProblem } = chordFrom(candidate.current);
    candidate.current.clear();
    setHeld([]);
    if (localProblem) {
      setProblem(localProblem);
      return;
    }
    if (sameChord(keys, value)) {
      stop();
      return;
    }
    setSaving(true);
    const error = await onSave(keys);
    setSaving(false);
    if (error) setProblem(error.message);
    else {
      setProblem(null);
      stop();
    }
  };

  const onKeyDown = (event: KeyboardEvent) => {
    event.preventDefault();
    event.stopPropagation();
    if (event.repeat) return;
    const key = virtualKey(event);
    if (key === VK.escape && down.current.size === 0) {
      setProblem(null);
      stop();
      return;
    }
    if (key === null) {
      setProblem("That key can't be used in a shortcut.");
      return;
    }
    down.current.add(key);
    candidate.current.add(key);
    setHeld([...candidate.current]);
  };

  const onKeyUp = (event: KeyboardEvent) => {
    event.preventDefault();
    const key = virtualKey(event);
    if (key !== null) down.current.delete(key);
    if (down.current.size === 0 && candidate.current.size > 0) void commit();
  };

  return (
    <div class="recorder">
      {recording ? (
        <div
          class="recorder-capture"
          tabIndex={0}
          ref={box}
          role="textbox"
          aria-label={`Press a new shortcut for ${label}. Escape cancels.`}
          onKeyDown={onKeyDown}
          onKeyUp={onKeyUp}
          onBlur={() => !saving && stop()}
        >
          {held.length ? <Keys keys={held} size="md" /> : <span class="recorder-hint">Press the new keys…</span>}
        </div>
      ) : (
        <div class="recorder-current">
          <Keys keys={value} size="md" />
          <Button
            size="sm"
            disabled={disabled}
            onClick={() => {
              setProblem(null);
              setRecording(true);
            }}
            aria-label={`Change ${label} shortcut`}
          >
            Change
          </Button>
        </div>
      )}
      {problem && (
        <p class="field-error" role="alert">
          {problem}
        </p>
      )}
    </div>
  );
}
