import { mountPlayer } from "./player";
import "./style.css";

const root = document.querySelector<HTMLElement>("#player");
if (!root) {
  throw new Error("player markup is missing");
}
const game = new URLSearchParams(location.search).get("game") === "charlie" ? "charlie" : "sample";
void mountPlayer(root, {
  game,
  basePath: import.meta.env.BASE_URL,
  page: true,
});
