// Vérifie la licence de chaque crate Rust (dépendances directes et indirectes,
// toutes plateformes confondues) à partir de `cargo metadata`.
// Principe 12 : MIT, Apache-2.0, BSD, ISC, Zlib, MPL-2.0. Les licences
// supplémentaires ci-dessous sont aussi permissives ; leur présence est
// justifiée dans docs/DECISIONS.md.
import { execFileSync } from "node:child_process";

const ALLOWED = new Set([
  "MIT",
  "MIT-0",
  "Apache-2.0",
  "Apache-2.0 WITH LLVM-exception",
  "BSD-2-Clause",
  "BSD-3-Clause",
  "ISC",
  "Zlib",
  "MPL-2.0",
  "0BSD",
  "Unicode-3.0",
]);

// Petit analyseur d'expressions SPDX : AND est prioritaire sur OR, parenthèses gérées.
function licenseOk(expr) {
  if (!expr) return false;
  const tokens = String(expr)
    .replace(/\s*\/\s*/g, " OR ")
    .replace(/\(/g, " ( ")
    .replace(/\)/g, " ) ")
    .trim()
    .split(/\s+/);
  let i = 0;
  const atom = () => {
    if (tokens[i] === "(") {
      i++;
      const v = or();
      i++; // « ) »
      return v;
    }
    let id = tokens[i++];
    if (tokens[i] === "WITH") {
      id += ` WITH ${tokens[i + 1]}`;
      i += 2;
    }
    return ALLOWED.has(id);
  };
  const and = () => {
    let v = atom();
    while (tokens[i] === "AND") {
      i++;
      v = atom() && v;
    }
    return v;
  };
  const or = () => {
    let v = and();
    while (tokens[i] === "OR") {
      i++;
      v = and() || v;
    }
    return v;
  };
  return or();
}

const json = execFileSync("cargo", ["metadata", "--format-version", "1", "--manifest-path", "src-tauri/Cargo.toml"], {
  encoding: "utf8",
  maxBuffer: 256 * 1024 * 1024,
});
const meta = JSON.parse(json);
const own = new Set(meta.workspace_members);
const packages = meta.packages.filter((p) => !own.has(p.id));
const bad = packages.filter((p) => !licenseOk(p.license));

const summary = new Map();
for (const p of packages) summary.set(p.license ?? "inconnue", (summary.get(p.license ?? "inconnue") ?? 0) + 1);
for (const [lic, n] of [...summary].sort((a, b) => b[1] - a[1])) console.log(`${String(n).padStart(4)}  ${lic}`);

if (bad.length) {
  console.error("\nLicences non permises :");
  for (const p of bad) console.error(`  ${p.name}@${p.version} — ${p.license ?? "inconnue"}`);
  process.exit(1);
}
console.log(`\n${packages.length} crates vérifiées : toutes les licences sont permises.`);
