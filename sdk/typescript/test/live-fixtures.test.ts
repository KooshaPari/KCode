/**
 * The live smoke scripts are the SDK's only coverage of real `launch()`
 * behaviour: they start an actual instance, drive a turn, and check isolation,
 * credentials, and every documented option. `npm run check` cannot run them,
 * because they need a built binary and a provider login.
 *
 * That gap is how they survived the kcode rebrand still shipping a bare
 * `"jcode"` binary default. `assertKcodePath` rejects that on sight, so
 * `node test/live-options.mjs` threw `cross_product_path` before it checked a
 * single documented option — and the child-daemon knobs were still spelled
 * `JCODE_NO_TELEMETRY` and friends, which the daemon no longer reads, so the
 * instances under test quietly ran with telemetry and memory enabled.
 *
 * These assertions run the guard against the values the scripts actually ship,
 * so the next rename fails here instead of in someone's terminal.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { assertKcodePath } from "../dist/product.js";

const here = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(here, "../../..");

const liveScripts = fs
  .readdirSync(here)
  .filter((name) => name.startsWith("live-") && name.endsWith(".mjs"))
  .sort();

/** `process.argv[2] ?? "kcode"`, and the `path.resolve(...)` form around it. */
const DEFAULT_BINARY = /process\.argv\[2\]\s*\?\?\s*"([^"]+)"/u;
const TEST_BINARY_ENV = /\bKCODE_SDK_TEST_BINARY\b/u;

function read(name: string): string {
  return fs.readFileSync(path.join(here, name), "utf8");
}

function defaultBinaryOf(source: string): string | undefined {
  return DEFAULT_BINARY.exec(source)?.[1];
}

/** Strip shell comments so prose explaining the old name is not read as code. */
function codeLines(script: string): string {
  return script
    .split("\n")
    .filter((line) => !line.trimStart().startsWith("#"))
    .join("\n");
}

/**
 * `assertKcodePath` resolves against the cwd and then walks up to the nearest
 * existing ancestor, so its verdict also depends on the directory the repo was
 * cloned into. Resolve from a scratch directory so these assertions are about
 * the value a script ships rather than about where someone cloned the repo.
 */
function assertGuardAccepts(label: string, candidate: string): void {
  const scratch = fs.mkdtempSync(path.join(os.tmpdir(), "kcode-guard-probe-"));
  const original = process.cwd();
  try {
    process.chdir(scratch);
    assert.doesNotThrow(
      () => assertKcodePath(candidate),
      `${label} hands assertKcodePath "${candidate}", which it rejects as a cross-product path`,
    );
  } finally {
    process.chdir(original);
    fs.rmSync(scratch, { recursive: true, force: true });
  }
}

test("the live smoke scripts are all discovered", () => {
  // Guards the filter itself: an empty or near-empty list would make every
  // assertion below vacuously true, which is how a rename could pass again.
  assert.ok(
    liveScripts.length >= 8,
    `expected the full live-* cohort, found only ${liveScripts.length}: ${liveScripts.join(", ")}`,
  );
});

test("exactly one live script takes its binary from the environment", () => {
  // live-tools.mjs is opt-in and insists on an absolute path; the rest either
  // default to a bare name or connect to a bridge test_sdk_e2e.sh started for
  // them. Pinning the count keeps a new env-driven script from appearing with
  // nobody checking the knob it reads.
  assert.deepEqual(liveScripts.filter((name) => TEST_BINARY_ENV.test(read(name))), ["live-tools.mjs"]);
});

for (const name of liveScripts) {
  test(`${name} ships a default binary the guard accepts`, () => {
    const source = read(name);
    const fallback = defaultBinaryOf(source);
    // A script with no argv default is driven by test_sdk_e2e.sh against a
    // bridge it already started (live-turn, live-control, live-capabilities)
    // and never names a binary, so there is no default left to get wrong.
    if (fallback !== undefined) assertGuardAccepts(name, fallback);
  });

  test(`${name} reads and sets only KCODE_-prefixed knobs`, () => {
    const legacy = [...read(name).matchAll(/\bJCODE_[A-Z_]+\b/gu)].map((m) => m[0]);
    assert.deepEqual(
      [...new Set(legacy)].sort(),
      [],
      `${name} still uses pre-rebrand env knobs the daemon no longer reads`,
    );
  });

  test(`${name} creates its scratch directories under a kcode prefix`, () => {
    // Only the ones that are actually handed to the SDK: a temp dir becomes an
    // instance home or a working directory, both of which go through the guard.
    for (const prefix of [...read(name).matchAll(/mkdtemp(?:Sync)?\(\s*path\.join\([^,]+,\s*"([^"]+)"/gu)].map((m) => m[1])) {
      assertGuardAccepts(`${name} temp prefix`, prefix);
    }
  });
}

test("the e2e driver builds the kcode binary, not the pre-rebrand name", () => {
  const driver = codeLines(fs.readFileSync(path.join(repoRoot, "scripts/test_sdk_e2e.sh"), "utf8"));
  assert.match(driver, /cargo build --profile selfdev -p kcode --bin kcode\b/u);
  assert.doesNotMatch(driver, /--bin jcode(?![\w-])/u, "the jcode bin target no longer exists in this fork");
});

test("the e2e driver's socket path is one the guard accepts", () => {
  const driver = fs.readFileSync(path.join(repoRoot, "scripts/test_sdk_e2e.sh"), "utf8");
  const socket = /socket="\$\{TMPDIR:-\/tmp\}\/([^"$]*)/u.exec(driver)?.[1];
  assert.ok(socket, "could not read the socket template out of test_sdk_e2e.sh");
  assertGuardAccepts("test_sdk_e2e.sh socket", socket);
});

test("the documented opt-in commands name a binary this fork builds", () => {
  const docs = [
    path.join(repoRoot, "sdk/typescript/README.md"),
    path.join(repoRoot, "docs/dev/GUY_ISSUE_ACCEPTANCE.md"),
  ];
  for (const doc of docs) {
    const source = fs.readFileSync(doc, "utf8");
    const stale = [...source.matchAll(/live-[a-z-]+\.mjs\s+\.\/target\/selfdev\/jcode\b/gu)];
    assert.deepEqual(
      stale.map((m) => m[0]),
      [],
      `${path.relative(repoRoot, doc)} still tells a reader to run a jcode binary that no longer exists`,
    );
  }
});
