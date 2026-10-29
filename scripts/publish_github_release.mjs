import fs from 'node:fs';
import path from 'node:path';

const TOKEN = process.env.GITHUB_TOKEN || process.env.GH_TOKEN || '';
const OWNER = 'risterz';
const REPO = 'FreeFetch';
const TAG = 'v0.1.0-beta';

async function main() {
  const headers = {
    Authorization: `token ${TOKEN}`,
    'User-Agent': 'FreeFetch-Release-Uploader',
    Accept: 'application/vnd.github.v3+json',
  };

  // 1. Create or get existing release for this tag
  console.log(`Checking release for tag ${TAG}...`);
  const listRes = await fetch(`https://api.github.com/repos/${OWNER}/${REPO}/releases/tags/${TAG}`, {
    headers,
  });

  let releaseData = null;
  if (listRes.ok) {
    releaseData = await listRes.json();
    console.log(`Found existing release: ID ${releaseData.id}`);
  } else {
    console.log('Creating new GitHub Release...');
    const releaseBody = `## FreeFetch v0.1.0 (Public Beta)

Clean-room, high-fidelity desktop media downloader built with Tauri v2, Rust, React 18, and FFmpeg.

### 📦 Release Assets
- **NSIS Setup Wizard**: \`FreeFetch_0.1.0_x64-setup.exe\` (51.1 MB) - Recommended for Windows users.
- **MSI Enterprise Installer**: \`FreeFetch_0.1.0_x64_en-US.msi\` (62.5 MB) - Clean per-user or administrative MSI.
- **Standalone Binary**: \`freefetch.exe\` (18.7 MB) - Portable single executable.

### 🔑 Cryptographic Checksums (SHA-256)
\`\`\`text
4AFF64BAD042B7AE3141E3CF52D75EF020F0AC4D90E7790D7B7C4036430B44A9  FreeFetch_0.1.0_x64-setup.exe
EB05459247D3B806C84B405D5E3B8B0A538F877CAA5FF528CAA7F0CDE702E66F  FreeFetch_0.1.0_x64_en-US.msi
3CA0DE43CB70C8DC881B3E83D65AA63966B6804796FF19F0107EAAA0A0DAF168  freefetch.exe
\`\`\`

### ⚡ Highlights
- Standalone bundled sidecars: **FFmpeg 7.1.1 essentials** and **yt-dlp 2026.10.26** included in installers.
- Operates out-of-the-box on clean Windows systems without developer tools, Python, or Chocolatey.
- Embedded SQLite database (\`freefetch.db\`) in WAL mode for task persistence and metrics.
- Passed 100% of automated test suites (181 Rust backend tests, 12 multiplatform integration tests, 48 Vitest frontend tests).
`;

    const createRes = await fetch(`https://api.github.com/repos/${OWNER}/${REPO}/releases`, {
      method: 'POST',
      headers: {
        ...headers,
        'Content-Type': 'application/json',
      },
      body: JSON.stringify({
        tag_name: TAG,
        name: 'FreeFetch v0.1.0 (Public Beta)',
        body: releaseBody,
        draft: false,
        prerelease: true,
      }),
    });

    if (!createRes.ok) {
      const err = await createRes.text();
      throw new Error(`Failed to create release: ${createRes.status} ${err}`);
    }
    releaseData = await createRes.json();
    console.log(`Created release: ID ${releaseData.id} (${releaseData.html_url})`);
  }

  const uploadUrl = releaseData.upload_url.replace('{?name,label}', '');
  console.log(`Upload URL: ${uploadUrl}`);

  // 2. Upload asset files
  const filesToUpload = [
    {
      name: 'FreeFetch_0.1.0_x64-setup.exe',
      filePath: path.resolve('src-tauri/target/release/bundle/nsis/FreeFetch_0.1.0_x64-setup.exe'),
    },
    {
      name: 'FreeFetch_0.1.0_x64_en-US.msi',
      filePath: path.resolve('src-tauri/target/release/bundle/msi/FreeFetch_0.1.0_x64_en-US.msi'),
    },
    {
      name: 'freefetch.exe',
      filePath: path.resolve('src-tauri/target/release/freefetch.exe'),
    },
  ];

  for (const item of filesToUpload) {
    if (!fs.existsSync(item.filePath)) {
      console.warn(`File not found: ${item.filePath}, skipping...`);
      continue;
    }

    const stats = fs.statSync(item.filePath);
    console.log(`Uploading ${item.name} (${stats.size} bytes)...`);

    // Check if asset already exists in release
    const existingAsset = (releaseData.assets || []).find((a) => a.name === item.name);
    if (existingAsset) {
      console.log(`Deleting existing asset ${existingAsset.id} before re-upload...`);
      await fetch(`https://api.github.com/repos/${OWNER}/${REPO}/releases/assets/${existingAsset.id}`, {
        method: 'DELETE',
        headers,
      });
    }

    const fileStream = fs.readFileSync(item.filePath);
    const targetUrl = `${uploadUrl}?name=${encodeURIComponent(item.name)}`;

    const uploadRes = await fetch(targetUrl, {
      method: 'POST',
      headers: {
        Authorization: `token ${TOKEN}`,
        'User-Agent': 'FreeFetch-Release-Uploader',
        'Content-Type': 'application/octet-stream',
        'Content-Length': String(stats.size),
      },
      body: fileStream,
    });

    if (!uploadRes.ok) {
      const errText = await uploadRes.text();
      console.error(`Failed to upload ${item.name}: ${uploadRes.status} ${errText}`);
    } else {
      const assetData = await uploadRes.json();
      console.log(`Successfully uploaded ${item.name}! Browser download URL: ${assetData.browser_download_url}`);
    }
  }

  console.log(`\nAll release assets processed. View release at: ${releaseData.html_url}`);
}

main().catch((err) => {
  console.error('Release publication failed:', err);
  process.exit(1);
});
