#!/usr/bin/env node
/**
 * Parse-check TypeScript/TSX without resolving workspace dependencies.
 *
 * This is intentionally a syntax gate, not a replacement for the Aedifex
 * workspace `check-types`/build. It uses the locally installed TypeScript
 * compiler API and reports only parser/transpile diagnostics. The full type
 * gate remains BLOCKED until the pinned upstream and Bun dependencies are
 * materialized.
 */
import { execFileSync } from "node:child_process";
import {
  existsSync,
  readFileSync,
  readdirSync,
  realpathSync,
  statSync,
} from "node:fs";
import { dirname, extname, join, resolve } from "node:path";
import { pathToFileURL } from "node:url";

function compilerPath() {
  const explicit = process.env.TYPESCRIPT_LIB;
  if (explicit && existsSync(explicit)) return explicit;
  const locator = process.platform === "win32" ? "where" : "which";
  const tsc = execFileSync(locator, ["tsc"], { encoding: "utf8" })
    .split(/\r?\n/)
    .find(Boolean);
  if (!tsc) throw new Error("tsc não encontrado");
  const real = realpathSync(tsc.trim());
  const candidate = join(dirname(dirname(real)), "lib", "typescript.js");
  if (!existsSync(candidate)) {
    throw new Error(`TypeScript compiler API ausente em ${candidate}`);
  }
  return candidate;
}

function collect(targets) {
  const files = [];
  const visit = (entry) => {
    const absolute = resolve(entry);
    const stat = statSync(absolute);
    if (stat.isDirectory()) {
      for (const name of readdirSync(absolute)) visit(join(absolute, name));
      return;
    }
    if ([".ts", ".tsx"].includes(extname(absolute))) files.push(absolute);
  };
  for (const target of targets) visit(target);
  return files.sort();
}

const targets = process.argv.slice(2);
if (!targets.length) targets.push("integrations/aedifex/overlay");

let ts;
try {
  const imported = await import(pathToFileURL(compilerPath()).href);
  ts = imported.default ?? imported;
} catch (error) {
  console.error(JSON.stringify({ files: 0, failures: [], blocked: String(error) }, null, 2));
  process.exit(2);
}

const failures = [];
const files = collect(targets);
for (const file of files) {
  const source = readFileSync(file, "utf8");
  const sourceFile = ts.createSourceFile(
    file,
    source,
    ts.ScriptTarget.ES2022,
    true,
    extname(file) === ".tsx" ? ts.ScriptKind.TSX : ts.ScriptKind.TS,
  );
  const result = ts.transpileModule(source, {
    fileName: file,
    reportDiagnostics: true,
    compilerOptions: {
      jsx: ts.JsxEmit.ReactJSX,
      target: ts.ScriptTarget.ES2022,
      module: ts.ModuleKind.ESNext,
      moduleResolution: ts.ModuleResolutionKind.Bundler,
      strict: true,
    },
  });
  for (const diagnostic of result.diagnostics ?? []) {
    if (diagnostic.category !== ts.DiagnosticCategory.Error) continue;
    const point = diagnostic.start == null
      ? null
      : ts.getLineAndCharacterOfPosition(sourceFile, diagnostic.start);
    failures.push({
      file,
      line: point ? point.line + 1 : null,
      column: point ? point.character + 1 : null,
      code: diagnostic.code,
      message: ts.flattenDiagnosticMessageText(diagnostic.messageText, "\n"),
    });
  }
}

console.log(JSON.stringify({ files: files.length, failures }, null, 2));
process.exit(failures.length ? 1 : 0);
