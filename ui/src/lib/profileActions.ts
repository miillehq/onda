import { ask } from "../api";
import { profile } from "./appState";
import { toastError } from "./notifications";

export async function setProfileName(name: string): Promise<boolean> {
  try {
    profile.value = await ask({ command: "setProfileName", args: { name: name.trim() || null } }, "profile");
    return true;
  } catch (error) {
    toastError(error, "Couldn't save your name");
    return false;
  }
}
export async function chooseProfilePicture(): Promise<void> {
  try {
    profile.value = await ask({ command: "chooseProfilePicture" }, "profile");
  } catch (error) {
    toastError(error, "Couldn't use that picture");
  }
}
export async function removeProfilePicture(): Promise<void> {
  try {
    profile.value = await ask({ command: "removeProfilePicture" }, "profile");
  } catch (error) {
    toastError(error, "Couldn't remove the picture");
  }
}
/** Hands a rendered PNG to native code, which asks where to save it. Null when cancelled. */
export async function saveStatsImage(pngBase64: string): Promise<string | null> {
  return ask({ command: "saveStatsImage", args: { pngBase64 } }, "savedFile").then((saved) => saved.path);
}
