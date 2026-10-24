import { route } from "../../lib/router";
import { About } from "./About";
import { AiPolish } from "./AiPolish";
import { AppStyles } from "./AppStyles";
import { General } from "./General";
import { Microphone } from "./Microphone";
import { Privacy } from "./Privacy";
import { Shortcuts } from "./Shortcuts";
import { Speech } from "./Speech";

export function SettingsView() {
  switch (route.value) {
    case "settings/shortcuts":
      return <Shortcuts />;
    case "settings/microphone":
      return <Microphone />;
    case "settings/speech":
      return <Speech />;
    case "settings/ai":
      return <AiPolish />;
    case "settings/apps":
      return <AppStyles />;
    case "settings/privacy":
      return <Privacy />;
    case "settings/about":
      return <About />;
    default:
      return <General />;
  }
}
