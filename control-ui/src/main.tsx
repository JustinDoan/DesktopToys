import { render } from "preact";
import { App } from "./app";
import { RoomConsole } from "./room";
import "./styles.css";

const surface = window.location.pathname.endsWith("room.html") || new URLSearchParams(window.location.search).get("surface") === "room"
  ? "room"
  : "pod";
document.documentElement.dataset.surface = surface;

render(surface === "room" ? <RoomConsole /> : <App />, document.getElementById("app")!);
