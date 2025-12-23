import { chmod, copyFile, mkdir, stat } from "node:fs/promises";
import { execFileSync } from "node:child_process";
import path from "node:path";
import process from "node:process";

const EXECUTABLE_MODE = 0o755;

function getRepoRoot() {
  return path.resolve(import.meta.dirname, "..", "..", "..");
}

function getPackageRoot() {
  return path.resolve(import.meta.dirname, "..");
}

function getLocalBinarySourcePath(repoRoot) {
  const binaryFileName =
    process.platform === "win32" ? "headlamp.exe" : "headlamp";
  return path.join(repoRoot, "target", "release", binaryFileName);
}

function getLocalPlatformKey() {
  const arch = process.arch;
  return `${process.platform}-${arch}`;
}

async function fileExists(filePath) {
  try {
    const fileStat = await stat(filePath);
    return fileStat.isFile();
  } catch {
    return false;
  }
}

async function stageLocalBinary() {
  const repoRoot = getRepoRoot();
  const packageRoot = getPackageRoot();

  execFileSync("cargo", ["build", "-q", "-p", "headlamp", "--release"], {
    cwd: repoRoot,
    stdio: "inherit",
  });

  const sourcePath = getLocalBinarySourcePath(repoRoot);
  const platformKey = getLocalPlatformKey();
  const binaryFileName = path.basename(sourcePath);
  const destinationDir = path.join(packageRoot, "bin", platformKey);
  const destinationPath = path.join(destinationDir, binaryFileName);

  if (!(await fileExists(sourcePath))) {
    process.stderr.write(
      `stage-binaries: missing local binary at ${sourcePath}\n` +
        `Tried: cargo build -p headlamp --release\n`
    );
    process.exit(1);
  }

  await mkdir(destinationDir, { recursive: true });
  await copyFile(sourcePath, destinationPath);
  if (process.platform !== "win32") {
    await chmod(destinationPath, EXECUTABLE_MODE);
  }
  process.stdout.write(`staged: ${destinationPath}\n`);
}

async function run() {
  const args = new Set(process.argv.slice(2));
  if (args.has("--local")) {
    await stageLocalBinary();
    return;
  }
  process.stderr.write("stage-binaries: supported flags: --local\n");
  process.exit(1);
}

await run();
