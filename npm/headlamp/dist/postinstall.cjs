const {
  chmodSync,
  createReadStream,
  createWriteStream,
  existsSync,
  mkdirSync,
  unlinkSync,
} = require("node:fs");
const path = require("node:path");
const https = require("node:https");
const zlib = require("node:zlib");

const EXECUTABLE_MODE = 0o755;

function getPackageRoot() {
  return path.resolve(__dirname, "..");
}

function getBinaryFileName() {
  return process.platform === "win32" ? "headlamp.exe" : "headlamp";
}

function isLikelyMusl() {
  if (process.platform !== "linux") return false;
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
  return path.join(
    getPackageRoot(),
    "bin",
    resolvePlatformKey(),
    getBinaryFileName()
  );
}

function tryChmodExecutable(filePath) {
  if (process.platform === "win32") return;
  try {
    chmodSync(filePath, EXECUTABLE_MODE);
  } catch {
    // best-effort
  }
}

function getPackageVersion() {
  // eslint-disable-next-line global-require, import/no-dynamic-require
  const packageJson = require(path.join(getPackageRoot(), "package.json"));
  return String(packageJson.version ?? "");
}

function buildDownloadUrl({ version, platformKey }) {
  const assetFileName = `${platformKey}-${getBinaryFileName()}.gz`;
  return `https://github.com/dbpiper/headlamp/releases/download/v${version}/${assetFileName}`;
}

function downloadToFile({ url, destinationFilePath }) {
  return new Promise((resolve, reject) => {
    const request = https.get(
      url,
      { headers: { "user-agent": "headlamp-npm-postinstall" } },
      (response) => {
        const statusCode = response.statusCode ?? 0;

        if (
          statusCode >= 300 &&
          statusCode < 400 &&
          response.headers.location
        ) {
          response.resume();
          downloadToFile({
            url: response.headers.location,
            destinationFilePath,
          }).then(resolve, reject);
          return;
        }

        if (statusCode !== 200) {
          response.resume();
          reject(
            new Error(
              `headlamp postinstall: download failed (${statusCode}) from ${url}`
            )
          );
          return;
        }

        const outputStream = createWriteStream(destinationFilePath);
        response.pipe(outputStream);
        outputStream.on("finish", resolve);
        outputStream.on("error", reject);
      }
    );
    request.on("error", reject);
  });
}

function gunzipToFile({ gzFilePath, outputFilePath }) {
  return new Promise((resolve, reject) => {
    mkdirSync(path.dirname(outputFilePath), { recursive: true });
    const inputStream = createReadStream(gzFilePath);
    const gunzipStream = zlib.createGunzip();
    const outputStream = createWriteStream(outputFilePath);

    inputStream.on("error", reject);
    gunzipStream.on("error", reject);
    outputStream.on("error", reject);
    outputStream.on("finish", resolve);

    inputStream.pipe(gunzipStream).pipe(outputStream);
  });
}

function safeUnlink(filePath) {
  try {
    unlinkSync(filePath);
  } catch {
    // ignore
  }
}

async function ensurePlatformBinary() {
  const binaryPath = resolveBinaryPath();
  if (existsSync(binaryPath)) {
    tryChmodExecutable(binaryPath);
    return;
  }

  const platformKey = resolvePlatformKey();
  const version = getPackageVersion();
  if (!version) {
    throw new Error("headlamp postinstall: missing package.json version");
  }

  const downloadUrl = buildDownloadUrl({ version, platformKey });
  const tempGzFilePath = path.join(
    getPackageRoot(),
    `.headlamp-${platformKey}.gz`
  );

  try {
    await downloadToFile({
      url: downloadUrl,
      destinationFilePath: tempGzFilePath,
    });
    await gunzipToFile({ gzFilePath: tempGzFilePath, outputFilePath: binaryPath });
    tryChmodExecutable(binaryPath);
  } finally {
    safeUnlink(tempGzFilePath);
  }
}

async function run() {
  try {
    await ensurePlatformBinary();
  } catch (error) {
    process.stderr.write(`${String(error)}\n`);
    process.exit(1);
  }
}

run();




