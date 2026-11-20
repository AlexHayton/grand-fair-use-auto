// Run test scripts through the simulation built for wasm, in Node.
//
//   node replay.mjs <pkg-dir> <data-root> <game> <hash-every> <script>...
//
// Prints each run's log and hashes exactly as `gfa-tool run` does, after a
// `== <script>` line. Game files are found case-insensitively, as the
// engine finds them.
import fs from "node:fs";
import path from "node:path";
import { createRequire } from "node:module";

const [pkg, root, game, hashEvery, ...scripts] = process.argv.slice(2);
const require = createRequire(import.meta.url);
const sim = require(path.resolve(pkg, "gfa_replay_wasm.js"));

// Every file under GTADATA/, by lower-case game path.
const files = new Map();
function walk(dir) {
  for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
    const p = path.join(dir, e.name);
    if (e.isDirectory()) walk(p);
    else files.set(path.relative(root, p).split(path.sep).join("/").toLowerCase(), p);
  }
}
walk(path.join(root, "GTADATA"));
const read = (gamePath) => {
  const p = files.get(gamePath.toLowerCase());
  if (!p) throw new Error(`no ${gamePath} in the data folder`);
  return fs.readFileSync(p);
};

const ini = {
  gta: "GTADATA/MISSION.INI",
  london69: "GTADATA/UK/MISSUK.INI",
  london61: "GTADATA/UK/MISSUKE.INI",
}[game];
const missionIni = read(ini);
for (const script of scripts) {
  const text = fs.readFileSync(script, "utf8");
  const m = text.match(/^#\s*section:\s*(\d+)/m);
  const section = m ? Number(m[1]) : 1;
  const map = read("GTADATA/" + sim.section_map(missionIni, section));
  console.log(`== ${path.basename(script)}`);
  process.stdout.write(
    sim.replay(map, missionIni, section, game === "gta", text, -1, Number(hashEvery)),
  );
}
