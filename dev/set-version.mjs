#!/usr/bin/env node
// AIMtrix's version lives in package.json, src-tauri/Cargo.toml (and its
// entry in Cargo.lock) and src-tauri/tauri.conf.json. Until AIMtrix goes
// public, builds are numbered <base>-<seconds since midnight UTC>, e.g.
// 0.0.1-52731; the base is package.json's version without any suffix.
//
//   node dev/set-version.mjs              # print the next version
//   node dev/set-version.mjs 0.0.1-52731  # write it into all three files
//
// Releases don't commit the version: the tag carries it, and the release
// workflow and dev/build-package.sh write it into their own copy.
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const path = (p) => join(root, p);

const pkg = JSON.parse(readFileSync(path("package.json"), "utf8"));
const base = pkg.version.split("-")[0];

/** The next build number: seconds since midnight UTC. */
function next() {
  const now = new Date();
  const seconds = now.getUTCHours() * 3600 + now.getUTCMinutes() * 60 + now.getUTCSeconds();
  return `${base}-${seconds}`;
}

const version = process.argv[2];
if (!version) {
  console.log(next());
  process.exit(0);
}
// SemVer, which Cargo and Tauri insist on: no leading zeros in numbers,
// including the numeric parts after the "-", and no empty parts.
const number = "(0|[1-9]\\d*)";
const part = `(${number}|\\d*[A-Za-z-][0-9A-Za-z-]*)`;
const semver = new RegExp(`^${number}\\.${number}\\.${number}(-${part}(\\.${part})*)?$`);
if (!semver.test(version)) {
  console.error(`Not a version: ${version}`);
  process.exit(1);
}

pkg.version = version;
writeFileSync(path("package.json"), JSON.stringify(pkg, null, 2) + "\n");

const conf = JSON.parse(readFileSync(path("src-tauri/tauri.conf.json"), "utf8"));
conf.version = version;
writeFileSync(path("src-tauri/tauri.conf.json"), JSON.stringify(conf, null, 2) + "\n");

// The first `version = ` is the [package] one.
const cargo = readFileSync(path("src-tauri/Cargo.toml"), "utf8");
writeFileSync(
  path("src-tauri/Cargo.toml"),
  cargo.replace(/^version = ".*"$/m, `version = "${version}"`),
);

// And AIMtrix's own entry in Cargo.lock, so builds with --locked (exactly
// the locked crates) don't refuse over it.
const lock = readFileSync(path("src-tauri/Cargo.lock"), "utf8");
const locked = lock.replace(/(\[\[package\]\]\nname = "aimtrix"\nversion = )".*"/, `$1"${version}"`);
if (locked === lock && !lock.includes(`name = "aimtrix"\nversion = "${version}"`)) {
  console.error("Couldn't find aimtrix in src-tauri/Cargo.lock");
  process.exit(1);
}
writeFileSync(path("src-tauri/Cargo.lock"), locked);

console.log(`Version ${version}`);
