// Vérifie que toutes les dépendances JavaScript livrées dans l'application
// (dépendances de production et leurs sous-dépendances) ont une licence permise
// par le principe 12 du cahier des charges. Les licences Rust sont vérifiées
// par cargo-deny (src-tauri/deny.toml).
import { readFileSync, existsSync } from "node:fs";
import { join } from "node:path";

const ALLOWED = new Set(["MIT", "Apache-2.0", "BSD-2-Clause", "BSD-3-Clause", "ISC", "Zlib", "MPL-2.0", "0BSD"]);

const root = process.cwd();
const pkg = JSON.parse(readFileSync(join(root, "package.json"), "utf8"));

function licenseOk(expr) {
  if (!expr) return false;
  // Accepte « A OR B » si au moins une est permise, « A AND B » si toutes le sont.
  const clean = String(expr).replace(/[()]/g, "").trim();
  if (clean.includes(" OR ")) return clean.split(" OR ").some((l) => licenseOk(l.trim()));
  if (clean.includes(" AND ")) return clean.split(" AND ").every((l) => licenseOk(l.trim()));
  return ALLOWED.has(clean);
}

const seen = new Map();
function visit(name, fromDir) {
  if (seen.has(name)) return;
  let dir = fromDir;
  let manifest = null;
  while (true) {
    const candidate = join(dir, "node_modules", name, "package.json");
    if (existsSync(candidate)) {
      manifest = JSON.parse(readFileSync(candidate, "utf8"));
      dir = join(dir, "node_modules", name);
      break;
    }
    const parent = join(dir, "..");
    if (parent === dir) break;
    dir = parent;
  }
  if (!manifest) return; // dépendance facultative non installée
  const license = typeof manifest.license === "object" ? manifest.license?.type : manifest.license;
  seen.set(name, { version: manifest.version, license });
  for (const dep of Object.keys(manifest.dependencies ?? {})) visit(dep, dir);
}

for (const dep of Object.keys(pkg.dependencies ?? {})) visit(dep, root);

const bad = [...seen].filter(([, info]) => !licenseOk(info.license));
for (const [name, info] of [...seen].sort()) console.log(`${name}@${info.version} — ${info.license}`);
if (bad.length) {
  console.error("\nLicences non permises :");
  for (const [name, info] of bad) console.error(`  ${name}@${info.version} — ${info.license ?? "inconnue"}`);
  process.exit(1);
}
console.log(`\n${seen.size} paquets vérifiés : toutes les licences sont permises.`);
