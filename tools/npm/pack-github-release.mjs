#!/usr/bin/env node

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import fs from "node:fs";
import path from "node:path";

const [outputArgument, ...packageArguments] = process.argv.slice(2);
const expectedVersion = process.env.BLUE_VIZE_VERSION;

if (!outputArgument || packageArguments.length === 0 || !expectedVersion) {
  throw new Error(
    "Usage: BLUE_VIZE_VERSION=<version> pack-github-release.mjs <output-dir> <package-dir>...",
  );
}

const outputDir = path.resolve(outputArgument);
fs.rmSync(outputDir, { recursive: true, force: true });
fs.mkdirSync(outputDir, { recursive: true });

const packages = [];
for (const packageArgument of packageArguments) {
  const packageDir = path.resolve(packageArgument);
  const packageJsonPath = path.join(packageDir, "package.json");
  const packageJson = JSON.parse(fs.readFileSync(packageJsonPath, "utf8"));

  assert.equal(
    packageJson.version,
    expectedVersion,
    `${packageJson.name} must use release version ${expectedVersion}`,
  );
  assertPublishableDependencies(packageJson, packageJsonPath);

  const result = spawnSync(
    process.env.NPM_BIN || "npm",
    ["pack", "--ignore-scripts", "--json", "--pack-destination", outputDir],
    {
      cwd: packageDir,
      encoding: "utf8",
      maxBuffer: 32 * 1024 * 1024,
    },
  );

  if (result.status !== 0) {
    throw new Error(
      [`npm pack failed for ${packageJson.name}`, result.stdout, result.stderr]
        .filter(Boolean)
        .join("\n"),
    );
  }

  const packResult = JSON.parse(result.stdout);
  assert.equal(packResult.length, 1, `expected one tarball for ${packageJson.name}`);
  const filename = packResult[0].filename;
  const tarballPath = path.join(outputDir, filename);
  const sha256 = createHash("sha256").update(fs.readFileSync(tarballPath)).digest("hex");

  packages.push({
    cpu: packageJson.cpu,
    filename,
    libc: packageJson.libc,
    name: packageJson.name,
    os: packageJson.os,
    sha256,
    size: fs.statSync(tarballPath).size,
    version: packageJson.version,
  });
}

packages.sort((left, right) => left.name.localeCompare(right.name));
fs.writeFileSync(
  path.join(outputDir, "SHA256SUMS"),
  `${packages.map(({ filename, sha256 }) => `${sha256}  ${filename}`).join("\n")}\n`,
);
fs.writeFileSync(
  path.join(outputDir, "release-manifest.json"),
  `${JSON.stringify(
    {
      sourceCommit: process.env.GITHUB_SHA || null,
      tag: process.env.GITHUB_REF_NAME || null,
      version: expectedVersion,
      packages,
    },
    null,
    2,
  )}\n`,
);

console.log(`Packed ${packages.length} packages into ${outputDir}`);

function assertPublishableDependencies(packageJson, packageJsonPath) {
  const unresolved = [];
  for (const section of [
    "dependencies",
    "optionalDependencies",
    "peerDependencies",
    "devDependencies",
  ]) {
    for (const [name, version] of Object.entries(packageJson[section] || {})) {
      if (typeof version === "string" && /^(catalog|workspace):/.test(version)) {
        unresolved.push(`${section}.${name}=${version}`);
      }
    }
  }
  assert.deepEqual(unresolved, [], `${packageJsonPath} has unresolved dependency protocols`);
}
