import { exec } from 'child_process';
import { createWriteStream } from 'fs';
import { copyFile, cp, mkdir, readdir, rename } from 'fs/promises';
import path from 'path';
import axios from 'axios';
import { ensureFile } from 'fs-extra';
import { extract } from 'tar';
import * as zl from 'zip-lib';
import { z } from 'zod';

enum Arch {
  x86_64_unknown_linux_gnu = 'x86_64-unknown-linux-gnu',
  aarch64_apple_darwin = 'aarch64-apple-darwin',
  x86_64_pc_windows_msvc = 'x86_64-pc-windows-msvc',
}
const envSchema = z.object({
  ARCH: z.nativeEnum(Arch),
  OLLAMA_VERSION: z.string().min(6),
  SHINKAI_NODE_VERSION: z.string().min(6),
});

type Env = z.infer<typeof envSchema>;

const env: Env = envSchema.parse(process.env);

const TEMP_PATH = './temp';
const OLLAMA_RESOURCES_PATH =
  './apps/shinkai-desktop/src-tauri/external-binaries/ollama/';
const SHINKAI_NODE_RESOURCES_PATH =
  './apps/shinkai-desktop/src-tauri/external-binaries/shinkai-node/';
const LLM_MODELS_PATH = './apps/shinkai-desktop/src-tauri/llm-models/';

const asBinaryName = (arch: Arch, path: string) => {
  return `${path}${arch === Arch.x86_64_pc_windows_msvc ? '.exe' : ''}`;
};

const asSidecarName = (arch: Arch, path: string) => {
  return path.replace(
    /(.exe){0,1}$/,
    `-${arch}${arch === Arch.x86_64_pc_windows_msvc ? '.exe' : ''}`,
  );
};

const addExecPermissions = (path: string) => {
  console.log(`Adding exec permissions (+x) to ${path}`);
  return exec(`chmod +x ${path}`);
};

const downloadFile = async (url: string, path: string): Promise<void> => {
  console.log(`Downloading ${url}`);
  const response = await axios({
    method: 'GET',
    url: url,
    responseType: 'stream',
    onDownloadProgress: function (progressEvent) {
      if (progressEvent.progress) {
        console.log(`${url} progress: ${progressEvent.progress * 100}%`);
      }
    },
  });
  console.log(`ensuring file ${path}`);
  await ensureFile(path);
  console.log(`creating write stream ${path}`);
  response.data.pipe(
    createWriteStream(path, { encoding: 'binary', flags: 'w' }),
  );
  await new Promise<void>((resolve, reject) => {
    response.data.on('end', () => {
      resolve();
    });
    response.data.on('error', (err: any) => {
      console.error(`Error downloading file: ${url}`, err);
      reject(err);
    });
  });
  console.log(`Download complete: ${url}`);
};

const downloadShinkaiNodeBinary = async (arch: Arch, version: string) => {
  console.log(`Downloading shinkai-node arch:${arch} version:${version}`);
  const downloadUrl = `https://download.shinkai.com/shinkai-node/binaries/production/${arch}/${version}.zip`;
  const zippedPath = path.join(TEMP_PATH, `shinkai-node-${version}.zip`);
  await downloadFile(downloadUrl, zippedPath);
  let unzippedPath = path.join(TEMP_PATH, `shinkai-node-${version}`);
  await zl.extract(zippedPath, unzippedPath, {
    overwrite: true,
  });

  const files = await readdir(unzippedPath);
  for (const file of files) {
    await cp(
      path.join(unzippedPath, file),
      path.join(SHINKAI_NODE_RESOURCES_PATH, file),
      {
        recursive: true,
      },
    );
  }

  // They are used as sidecars in Tauri
  const shinkaiNodeBinaryPath = asBinaryName(
    arch,
    `./apps/shinkai-desktop/src-tauri/external-binaries/shinkai-node/shinkai-node`,
  );
  const shinkaiToolsRunnerDenoBinaryPath = asBinaryName(
    arch,
    `./apps/shinkai-desktop/src-tauri/external-binaries/shinkai-node/shinkai-tools-runner-resources/deno`,
  );
  const shinkaiToolsRunnerUvBinaryPath = asBinaryName(
    arch,
    `./apps/shinkai-desktop/src-tauri/external-binaries/shinkai-node/shinkai-tools-runner-resources/uv`,
  );
  await rename(
    shinkaiNodeBinaryPath,
    asSidecarName(arch, shinkaiNodeBinaryPath),
  );
  await rename(
    shinkaiToolsRunnerDenoBinaryPath,
    asSidecarName(arch, shinkaiToolsRunnerDenoBinaryPath),
  );
  await rename(
    shinkaiToolsRunnerUvBinaryPath,
    asSidecarName(arch, shinkaiToolsRunnerUvBinaryPath),
  );

  await addExecPermissions(asSidecarName(arch, shinkaiNodeBinaryPath));
  await addExecPermissions(
    asSidecarName(arch, shinkaiToolsRunnerDenoBinaryPath),
  );
  await addExecPermissions(asSidecarName(arch, shinkaiToolsRunnerUvBinaryPath));
};

const downloadOllamaAarch64AppleDarwin = async (version: string) => {
  const downloadUrl = `https://github.com/ollama/ollama/releases/download/${version}/Ollama-darwin.zip`;
  const zippedPath = path.join(
    TEMP_PATH,
    `ollama-${Arch.aarch64_apple_darwin}.zip`,
  );
  await downloadFile(downloadUrl, zippedPath);
  const unzippedPath = path.join(
    TEMP_PATH,
    `ollama-${Arch.aarch64_apple_darwin}-${version}`,
  );
  await zl.extract(zippedPath, unzippedPath);
  const ollamaBinaryPath = asSidecarName(
    Arch.aarch64_apple_darwin,
    `./apps/shinkai-desktop/src-tauri/external-binaries/ollama/ollama`,
  );
  await ensureFile(ollamaBinaryPath);
  await copyFile(
    path.join(unzippedPath, 'Ollama.app/Contents/Resources/ollama'),
    ollamaBinaryPath,
  );
  await addExecPermissions(ollamaBinaryPath);
};

const downloadOllamax8664UnknownLinuxGnu = async (version: string) => {
  let downloadUrl = `https://github.com/ollama/ollama/releases/download/${version}/ollama-linux-amd64.tgz`;
  const zippedPath = path.join(TEMP_PATH, `ollama-linux-amd64-${version}.tgz`);

  await downloadFile(downloadUrl, zippedPath);

  const unzippedPath = path.join(TEMP_PATH, `ollama-linux-amd64-${version}`);

  await mkdir(unzippedPath, { recursive: true });
  await extract({
    f: zippedPath,
    C: unzippedPath,
    strip: 0,
  });
  const ollamaBinaryPath = asSidecarName(
    Arch.x86_64_unknown_linux_gnu,
    `./apps/shinkai-desktop/src-tauri/external-binaries/ollama/ollama`,
  );
  await ensureFile(ollamaBinaryPath);
  await copyFile(path.join(unzippedPath, 'bin/ollama'), ollamaBinaryPath);
  await addExecPermissions(ollamaBinaryPath);
};

const downloadOllamax8664PcWindowsMsvc = async (version: string) => {
  await downloadOllamax8664PcWindowsMsvcNvidia(version);
  await downloadOllamax8664PcWindowsMsvcRocm(version);
};
const downloadOllamax8664PcWindowsMsvcRocm = async (version: string) => {
  const downloadUrl = `https://github.com/ollama/ollama/releases/download/${version}/ollama-windows-amd64-rocm.zip`;
  const zippedPath = path.join(
    TEMP_PATH,
    `ollama-windows-amd64-rocm-${version}.zip`,
  );
  await downloadFile(downloadUrl, zippedPath);
  const unzippedPath = path.join(
    TEMP_PATH,
    `ollama-windows-amd64-rocm-${version}`,
  );
  await zl.extract(zippedPath, unzippedPath);
  const files = await readdir(unzippedPath);
  await cp(
    path.join(unzippedPath, 'lib', 'ollama', 'rocm'),
    path.join(OLLAMA_RESOURCES_PATH, 'lib', 'rocm'),
    {
      recursive: true,
    },
  );
};
const downloadOllamax8664PcWindowsMsvcNvidia = async (version: string) => {
  const downloadUrl = `https://github.com/ollama/ollama/releases/download/${version}/ollama-windows-amd64.zip`;
  const zippedPath = path.join(
    TEMP_PATH,
    `ollama-windows-amd64-${version}.zip`,
  );

  await downloadFile(downloadUrl, zippedPath);

  const unzippedPath = path.join(TEMP_PATH, `ollama-windows-amd64-${version}`);
  await zl.extract(zippedPath, unzippedPath);

  const files = await readdir(unzippedPath);
  for (const file of files) {
    await cp(
      path.join(unzippedPath, file),
      path.join(OLLAMA_RESOURCES_PATH, file),
      {
        recursive: true,
      },
    );
  }

  const ollamaBinaryPath = asBinaryName(
    Arch.x86_64_pc_windows_msvc,
    `./apps/shinkai-desktop/src-tauri/external-binaries/ollama/ollama`,
  );
  await rename(
    ollamaBinaryPath,
    asSidecarName(Arch.x86_64_pc_windows_msvc, ollamaBinaryPath),
  );
  await addExecPermissions(
    asSidecarName(Arch.x86_64_pc_windows_msvc, ollamaBinaryPath),
  );
};

const downloadOllama = {
  [Arch.aarch64_apple_darwin]: downloadOllamaAarch64AppleDarwin,
  [Arch.x86_64_unknown_linux_gnu]: downloadOllamax8664UnknownLinuxGnu,
  [Arch.x86_64_pc_windows_msvc]: downloadOllamax8664PcWindowsMsvc,
};

const downloadEmbeddingModel = async () => {
  console.log(`Downloading embedding model`);
  const downloadUrl = `https://download.shinkai.com/llm-models/embeddinggemma-300M-BF16.gguf`;
  await downloadFile(
    downloadUrl,
    path.join(LLM_MODELS_PATH, 'embeddinggemma-300M-BF16.gguf'),
  );
};

export const main = async () => {
  await downloadShinkaiNodeBinary(env.ARCH, env.SHINKAI_NODE_VERSION);
  await downloadOllama[env.ARCH](env.OLLAMA_VERSION);
  await downloadEmbeddingModel();
};

main().catch(console.error);
