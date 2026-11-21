import { useEffect, useState } from "preact/hooks";
import { ask } from "../api";
import type { CloudModel } from "../generated/api";
import { activeProfile, chatgptReady, cloudModels } from "./store";

const requests = new Map<string, Promise<CloudModel[]>>();

function accountModels(profileId: string): Promise<CloudModel[]> {
  const pending = requests.get(profileId);
  if (pending) return pending;
  const request = ask({ command: "listAvailableModels" }, "cloudModels")
    .finally(() => requests.delete(profileId));
  requests.set(profileId, request);
  return request;
}

/** Use the current ChatGPT account's model list in Settings and in rewrite preferences. */
export function useCloudModels(): { models: CloudModel[]; loading: boolean } {
  const [loading, setLoading] = useState(false);
  const profileId = activeProfile.value?.id;
  const ready = chatgptReady.value;
  useEffect(() => {
    if (!ready || !profileId || cloudModels.value) {
      setLoading(false);
      return;
    }
    let mounted = true;
    setLoading(true);
    void accountModels(profileId)
      .then((models) => {
        if (activeProfile.value?.id === profileId) cloudModels.value = models;
      })
      // Leave the cache empty so reopening the editor can recover from a temporary outage.
      .catch(() => undefined)
      .finally(() => { if (mounted) setLoading(false); });
    return () => { mounted = false; };
  }, [profileId, ready]);
  return { models: cloudModels.value ?? [], loading };
}
