#!/usr/bin/env node

const { spawnSync } = require("node:child_process");
const { existsSync } = require("node:fs");
const path = require("node:path");

function getPackageRoot() {
  return path.resolve(__dirname, "..");
}

function getBinaryFileName() {
  return process.platform === "win32" ? "headlamp.exe" : "headlamp";
}

function isLikelyMusl() {
  if (process.platform !== "linux") return false;
  // Node emits glibc version in the diagnostic report when dynamically linked against glibc.
  // On musl builds this is generally absent.
  try {
    const report = process.report?.getReport?.();
    const glibcVersion = report?.header?.glibcVersionRuntime;
    return !glibcVersion;
  } catch {
    return false;
  }
}

function resolvePlatformKey() {
  const arch = process.arch;
  if (process.platform === "linux") {
    const libc = isLikelyMusl() ? "musl" : "gnu";
    return `linux-${arch}-${libc}`;
  }
  return `${process.platform}-${arch}`;
}

function resolveBinaryPath() {
  const packageRoot = getPackageRoot();
  const platformKey = resolvePlatformKey();
  const binaryFileName = getBinaryFileName();
  return path.join(packageRoot, "bin", platformKey, binaryFileName);
}

function failMissingBinary(binaryPath) {
  const platformKey = resolvePlatformKey();
  const availableHint = "This npm package is expected to include all platform binaries. If you're developing locally, run `npm run stage:local` in the package directory.";
  const message = [
    `headlamp: no bundled binary for ${platformKey}`,
    `expected: ${binaryPath}`,
    availableHint,
  ].join("\n");
  process.stderr.write(`${message}\n`);
  process.exit(1);
}

function run() {
  const binaryPath = resolveBinaryPath();
  if (!existsSync(binaryPath)) {
    failMissingBinary(binaryPath);
    return;
  }

  const result = spawnSync(binaryPath, process.argv.slice(2), {
    stdio: "inherit",
  });

  if (result.error) {
    process.stderr.write(`${String(result.error)}\n`);
    process.exit(1);
  }

  process.exit(result.status ?? 1);
}

run();




