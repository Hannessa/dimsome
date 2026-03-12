import fs from "node:fs";
import path from "node:path";

const workspaceRoot = process.cwd();
const tauriConfigPath = path.join(workspaceRoot, "src-tauri", "tauri.conf.json");
const cargoManifestPath = path.join(workspaceRoot, "src-tauri", "Cargo.toml");
const releaseDir = path.join(workspaceRoot, "release");

const tauriConfig = JSON.parse(fs.readFileSync(tauriConfigPath, "utf8"));
const cargoManifest = fs.readFileSync(cargoManifestPath, "utf8");

const version = tauriConfig.version;
const productName = tauriConfig.productName;
const packageNameMatch = cargoManifest.match(/^name\s*=\s*"([^"]+)"/m);

if (!version || !productName || !packageNameMatch) {
  throw new Error("Could not determine release artifact naming metadata.");
}

const binaryName = `${packageNameMatch[1]}.exe`;
const portableExeName = `${productName}_${version}_x64-portable.exe`;
const installerName = `${productName}_${version}_x64-setup.exe`;

const sourceExePath = path.join(workspaceRoot, "src-tauri", "target", "release", binaryName);
const sourceInstallerPath = path.join(workspaceRoot, "src-tauri", "target", "release", "bundle", "nsis", installerName);
const portableExePath = path.join(releaseDir, portableExeName);
const installerPath = path.join(releaseDir, installerName);

if (!fs.existsSync(sourceExePath)) {
  throw new Error(`Expected release executable was not found: ${sourceExePath}`);
}

if (!fs.existsSync(sourceInstallerPath)) {
  throw new Error(`Expected installer was not found: ${sourceInstallerPath}`);
}

fs.mkdirSync(releaseDir, { recursive: true });
fs.copyFileSync(sourceExePath, portableExePath);
fs.copyFileSync(sourceInstallerPath, installerPath);

console.log(`Portable artifact ready: ${portableExePath}`);
console.log(`Installer artifact ready: ${installerPath}`);
