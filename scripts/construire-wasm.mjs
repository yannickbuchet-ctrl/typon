// Compile le moteur Rust (core/) en WebAssembly et le copie dans src/wasm/, d'où Vite le sert.
//
//   node scripts/construire-wasm.mjs
//
// Il faut Rust et la cible wasm32-unknown-unknown (rustup target add wasm32-unknown-unknown). Sur ce PC, cargo est
// dans %USERPROFILE%\.cargo\bin, même s'il n'est pas sur le PATH.
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";

const racine = path.resolve(import.meta.dirname, "..");
const cargoBin = path.join(os.homedir(), ".cargo", "bin");
const env = { ...process.env, PATH: `${cargoBin}${path.delimiter}${process.env.PATH ?? ""}` };
const r = spawnSync("cargo", ["build", "--release", "--target", "wasm32-unknown-unknown"], { cwd: path.join(racine, "core"), env, stdio: "inherit", shell: process.platform === "win32" });
if (r.status !== 0) {
  console.error("La compilation du moteur Rust a échoué.");
  process.exit(r.status ?? 1);
}
const source = path.join(racine, "core", "target", "wasm32-unknown-unknown", "release", "typon_core.wasm");
const cible = path.join(racine, "src", "wasm", "typon_core.wasm");
fs.mkdirSync(path.dirname(cible), { recursive: true });
fs.copyFileSync(source, cible);
console.log(`Moteur WebAssembly : ${(fs.statSync(cible).size / 1024).toFixed(0)} Ko → src/wasm/typon_core.wasm`);
