import { readFileSync, writeFileSync } from "node:fs";
import { Resvg } from "@resvg/resvg-js";

const svg = readFileSync(new URL("../design/mark.svg", import.meta.url));
const png = new Resvg(svg, { fitTo: { mode: "width", value: 1024 } })
  .render()
  .asPng();
writeFileSync(new URL("../app-icon.png", import.meta.url), png);
writeFileSync(new URL("../public/mark.png", import.meta.url), png);
console.log("rendered app-icon.png");
