import "@fontsource-variable/inter";
import "../styles/tokens.css";
import "../styles/pill.css";
import { render } from "preact";
import { MockBadge } from "../components/MockBadge";
import { Pill, startPill } from "./Pill";

const root = document.getElementById("pill");
if (!root) throw new Error("Pill root is missing.");
render(
  <>
    <Pill />
    <MockBadge />
  </>,
  root,
);
void startPill();
