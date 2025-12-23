const { chmodSync, readdirSync } = require("node:fs");
const path = require("node:path");

const EXECUTABLE_MODE = 0o755;

function listBinaryFiles(binRoot) {
  if (!binRoot) return [];
  try {
    return readdirSync(binRoot, { withFileTypes: true })
      .flatMap((entry) => {
        if (!entry.isDirectory()) return [];
        const folder = path.join(binRoot, entry.name);
        return readdirSync(folder, { withFileTypes: true })
          .filter((subEntry) => subEntry.isFile())
          .map((subEntry) => path.join(folder, subEntry.name));
      });
  } catch {
    return [];
  }
}

function tryChmodExecutable(filePath) {
  if (process.platform === "win32") return;
  try {
    chmodSync(filePath, EXECUTABLE_MODE);
  } catch {
    // best-effort
  }
}

function run() {
  const packageRoot = path.resolve(__dirname, "..");
  const binRoot = path.join(packageRoot, "bin");
  listBinaryFiles(binRoot).forEach(tryChmodExecutable);
}

run();


