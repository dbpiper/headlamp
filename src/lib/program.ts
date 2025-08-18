/* eslint-disable no-continue */
/* eslint-disable import/no-extraneous-dependencies */
import * as path from 'node:path';
import * as os from 'node:os';
import * as fsSync from 'node:fs';
import * as fs from 'node:fs/promises';

import * as LibReport from 'istanbul-lib-report';
import * as Reports from 'istanbul-reports';
import { createCoverageMap } from 'istanbul-lib-coverage';

import { safeEnv } from './env-utils';
import { runExitCode, runText, runWithCapture } from './_exec';
import { deriveArgs } from './args';
import {
  findRepoRoot,
  argsForDiscovery,
  discoverJestResilient,
  filterCandidatesForProject,
  decideShouldRunJest,
  discoverJestCached,
} from './discovery';
import { readCoverageJson, filterCoverageMap } from './coverage-core';
import {
  printPerFileCompositeTables,
  printCompactCoverage,
  printDetailedCoverage,
} from './coverage-print';
import { JEST_BRIDGE_REPORTER_SOURCE } from './jest-reporter-source';
import { JEST_BRIDGE_ENV_SOURCE } from './jest-environment-source';
import { formatJestOutputVitest } from './formatJestOutputVitest';
import {
  renderVitestFromJestJSON,
  coerceJestJsonToBridge,
  type BridgeJSON,
} from './formatter/bridge';
import { makeCtx } from './formatter/context';
import { stripAnsiSimple } from './stacks';
import { tintPct } from './bars';
import { selectDirectTestsForProduction } from './graph-distance';
import { computeDirectnessRank, sortTestResultsWithRank } from './relevance';
import { runParallelStride } from './parallel';
import { loadHeadlampConfig, type HeadlampConfig } from './config';

const jestBin = './node_modules/.bin/jest';

export const registerSignalHandlersOnce = () => {
  let handled = false;
  const on = (sig: NodeJS.Signals) => {
    if (handled) {
      return;
    }
    handled = true;
    process.stdout.write(`\nReceived ${sig}, exiting...\n`);
    process.exit(130);
  };
  process.once('SIGINT', on);
  process.once('SIGTERM', on);
};

const isDebug = (): boolean =>
  Boolean((process.env as unknown as { TEST_CLI_DEBUG?: string }).TEST_CLI_DEBUG);

export const mergeLcov = async (): Promise<void> => {
  const vitestLcovPath = 'coverage/vitest/lcov.info';
  const mergedOutPath = 'coverage/lcov.info';
  const readOrEmpty = async (filePath: string) => {
    try {
      return await (await import('node:fs/promises')).readFile(filePath, 'utf8');
    } catch {
      return '';
    }
  };
  let vitestContent = '';
  const jestParts: string[] = [];
  try {
    vitestContent = await readOrEmpty(vitestLcovPath);
  } catch (readVitestError) {
    if (isDebug()) {
      console.info(`read vitest lcov failed: ${String(readVitestError)}`);
    }
  }
  // Merge all lcov.info files under coverage/jest/** (including root)
  const collectLcovs = (dir: string): string[] => {
    const out: string[] = [];
    try {
      const entries = fsSync.readdirSync(dir, { withFileTypes: true });
      for (const entry of entries) {
        const full = path.join(dir, entry.name);
        if (entry.isDirectory()) {
          out.push(...collectLcovs(full));
        } else if (entry.isFile() && entry.name === 'lcov.info') {
          out.push(full);
        }
      }
    } catch {
      /* ignore */
    }
    return out;
  };
  try {
    const jestRoot = path.join('coverage', 'jest');
    const candidates = [path.join(jestRoot, 'lcov.info'), ...collectLcovs(jestRoot)]
      .map((candidatePath) => path.resolve(candidatePath))
      .filter((absolutePath, index, arr) => arr.indexOf(absolutePath) === index);
    for (const filePath of candidates) {
      try {
        // eslint-disable-next-line no-await-in-loop
        const content = await readOrEmpty(filePath);
        if (content.trim()) {
          jestParts.push(content.trim());
        }
      } catch {
        /* ignore */
      }
    }
  } catch (readJestError) {
    if (isDebug()) {
      console.info(`scan jest lcov failed: ${String(readJestError)}`);
    }
  }
  if (!vitestContent && jestParts.length === 0) {
    if (isDebug()) {
      console.info('No coverage outputs found to merge.');
    }
    return;
  }
  const merged = [vitestContent.trim(), ...jestParts].filter(Boolean).join('\n');
  if (merged.length > 0) {
    await (await import('node:fs/promises')).mkdir('coverage', { recursive: true });
    await (await import('node:fs/promises')).writeFile(mergedOutPath, `${merged}\n`, 'utf8');
    if (isDebug()) {
      console.info(`Merged coverage written to ${mergedOutPath}`);
    }
  } else if (isDebug()) {
    console.info('Coverage files existed but were empty; nothing to merge.');
  }
};

export const emitMergedCoverage = async (
  ui: 'jest' | 'both',
  opts: {
    readonly selectionSpecified: boolean;
    readonly selectionPaths: readonly string[];
    readonly includeGlobs: readonly string[];
    readonly excludeGlobs: readonly string[];
    readonly workspaceRoot?: string;
    readonly editorCmd?: string;
    readonly coverageDetail?: number | 'all' | 'auto';
    readonly coverageShowCode?: boolean;
    readonly coverageMode?: 'compact' | 'full' | 'auto';
    readonly coverageMaxFiles?: number;
    readonly coverageMaxHotspots?: number;
    readonly coveragePageFit?: boolean;
    readonly executedTests?: readonly string[];
  },
): Promise<void> => {
  // Merge any coverage-final.json under coverage/jest/**
  const map = createCoverageMap({});
  const listJsons = (dir: string): string[] => {
    const out: string[] = [];
    try {
      const entries = fsSync.readdirSync(dir, { withFileTypes: true });
      for (const entry of entries) {
        const full = path.join(dir, entry.name);
        if (entry.isDirectory()) {
          out.push(...listJsons(full));
        } else if (entry.isFile() && entry.name === 'coverage-final.json') {
          out.push(full);
        }
      }
    } catch {
      /* ignore */
    }
    return out;
  };
  const coverageRoot = path.join('coverage', 'jest');
  const jsonCandidates = [
    path.join(coverageRoot, 'coverage-final.json'),
    ...listJsons(coverageRoot),
  ]
    .map((candidatePath) => path.resolve(candidatePath))
    .filter((absolutePath, index, arr) => {
      const isFirst = arr.indexOf(absolutePath) === index;
      const exists = fsSync.existsSync(absolutePath);
      return isFirst && exists;
    });
  for (const jsonPath of jsonCandidates) {
    try {
      // eslint-disable-next-line no-await-in-loop
      const data = await readCoverageJson(jsonPath);
      if (Object.keys(data).length) {
        map.merge(data);
      }
    } catch (mergeJestError) {
      console.warn(`Failed merging jest coverage JSON @ ${jsonPath}: ${String(mergeJestError)}`);
    }
  }
  if (map.files().length === 0) {
    if (isDebug()) {
      console.info('No JSON coverage to merge; skipping merged coverage print.');
    }
    return;
  }

  const repoRoot = opts.workspaceRoot ?? (await findRepoRoot());

  let filteredMap = filterCoverageMap(map, {
    includes: opts.includeGlobs,
    excludes: opts.excludeGlobs,
    root: repoRoot,
    selectionSpecified: Boolean(opts.selectionSpecified),
  });
  if (filteredMap.files().length === 0) {
    console.warn(
      'Coverage filters matched 0 files; retrying with include=**/* to avoid empty output.',
    );
    filteredMap = filterCoverageMap(map, {
      includes: ['**/*'],
      excludes: opts.excludeGlobs,
      root: repoRoot,
      selectionSpecified: Boolean(opts.selectionSpecified),
    });
    if (filteredMap.files().length === 0) {
      console.info('Coverage present but still no matches; skipping print.');
      return;
    }
  }

  let changedFilesOutput: readonly string[] = [];

  try {
    const out = await runText('git', ['diff', '--name-only', '--diff-filter=ACMRTUXB', 'HEAD'], {
      env: safeEnv(process.env, {}) as unknown as NodeJS.ProcessEnv,
    });
    changedFilesOutput = out
      .split(/\r?\n/)
      .map((line) => line.trim())
      .filter(Boolean)
      .map((filePathText) => filePathText.replace(/\\/g, '/'));
  } catch (gitError) {
    console.warn(`git diff failed when deriving changed files: ${String(gitError)}`);
  }

  await printPerFileCompositeTables({
    map: filteredMap,
    root: repoRoot,
    pageFit: opts.coveragePageFit ?? Boolean(process.stdout.isTTY),
    ...(opts.coverageMaxHotspots !== undefined ? { maxHotspots: opts.coverageMaxHotspots } : {}),
    selectionPaths: opts.selectionPaths ?? [],
    changedFiles: changedFilesOutput,
    executedTests: opts.executedTests ?? [],
  });

  const context = LibReport.createContext({
    dir: path.resolve('coverage', 'merged'),
    coverageMap: filteredMap,
    defaultSummarizer: 'nested',
  });

  const reporters =
    ui === 'jest'
      ? [Reports.create('text', { file: 'coverage.txt' })]
      : [
          Reports.create('text', { file: 'coverage.txt' }),
          Reports.create('text-summary', { file: 'coverage-summary.txt' }),
        ];

  const colorizeIstanbulLine = (lineText: string): string => {
    const separator = /^[-=\s]+$/;
    if (separator.test(lineText.trim())) {
      return lineText;
    }
    const headerLike = /\bFile\b\s*\|\s*%\s*Stmts\b/.test(lineText);
    if (headerLike) {
      return lineText;
    }
    if (/^\s*(Statements|Branches|Functions|Lines)\s*:/.test(lineText)) {
      // Color the label, percentage, and the raw counts in parens, e.g. ( 822/1816 )
      const updated = lineText.replace(
        /(Statements|Branches|Functions|Lines)(\s*:\s*)(\d+(?:\.\d+)?)(%)/,
        (_m, label, sep, num, pct) => {
          const colorize = tintPct(Number(num));
          return `${colorize(label)}${sep}${colorize(`${num}${pct}`)}`;
        },
      );
      return updated.replace(/\(\s*(\d+)\s*\/\s*(\d+)\s*\)/, (_match, coveredText, totalText) => {
        const percent = (() => {
          const totalCount = Number(totalText);
          const coveredCount = Number(coveredText);
          return totalCount > 0 ? (coveredCount / totalCount) * 100 : 0;
        })();
        const colorize = tintPct(percent);
        return colorize(`( ${coveredText}/${totalText} )`);
      });
    }
    if (lineText.includes('|')) {
      const parts = lineText.split('|');
      if (parts.length >= 2) {
        // Compute row weight from numeric percent columns
        const numericValues: number[] = [];
        for (let index = 1; index < parts.length - 1; index += 1) {
          const value = Number((parts[index] ?? '').trim());
          if (!Number.isNaN(value) && value >= 0 && value <= 100) {
            numericValues.push(value);
          }
        }
        const rowWeight = numericValues.length ? Math.min(...numericValues) : undefined;
        // Tint each numeric % column
        for (let index = 1; index < parts.length - 1; index += 1) {
          const raw = parts[index] ?? '';
          const value = Number(raw.trim());
          if (!Number.isNaN(value) && value >= 0 && value <= 100) {
            parts[index] = tintPct(value)(raw);
          }
        }
        // Tint the File/Group label and Uncovered column based on row weight
        if (rowWeight !== undefined) {
          parts[0] = tintPct(rowWeight)(parts[0] ?? '');
          const lastIndex = parts.length - 1;
          if (lastIndex >= 1) {
            parts[lastIndex] = tintPct(rowWeight)(parts[lastIndex] ?? '');
          }
        }
        return parts.join('|');
      }
    }
    return lineText;
  };
  for (const reporter of reporters) {
    reporter.execute(context);
  }
  const textPath = path.resolve('coverage', 'merged', 'coverage.txt');
  const summaryPath = path.resolve('coverage', 'merged', 'coverage-summary.txt');
  const filesToPrint: string[] = [];
  if (fsSync.existsSync(textPath)) {
    filesToPrint.push(textPath);
  }
  if (fsSync.existsSync(summaryPath)) {
    filesToPrint.push(summaryPath);
  }
  if (filesToPrint.length > 0) {
    for (const filePath of filesToPrint) {
      try {
        const raw = fsSync.readFileSync(filePath, 'utf8');
        const colored = raw.split(/\r?\n/).map(colorizeIstanbulLine).join('\n');
        process.stdout.write(colored.endsWith('\n') ? colored : `${colored}\n`);
      } catch {
        // fall back to raw printing
        try {
          const raw = fsSync.readFileSync(filePath, 'utf8');
          process.stdout.write(raw.endsWith('\n') ? raw : `${raw}\n`);
        } catch {
          /* ignore */
        }
      }
    }
  } else {
    // Fallback: no files created by reporter; run standard reporters to stdout (uncolored)
    const stdoutReporters =
      ui === 'jest'
        ? [Reports.create('text', {})]
        : [Reports.create('text', {}), Reports.create('text-summary', {})];
    for (const reporter of stdoutReporters) {
      reporter.execute(context);
    }
  }

  // Optional deep-dive per-file coverage: only when explicitly requested (not on 'auto')
  const modeResolved: 'compact' | 'full' =
    opts.coverageMode && opts.coverageMode !== 'auto' ? opts.coverageMode : 'full';
  const shouldDeepDive = opts.coverageDetail !== undefined && opts.coverageDetail !== 'auto';
  if (shouldDeepDive) {
    if (modeResolved === 'compact') {
      await printCompactCoverage({
        map: filteredMap,
        root: repoRoot,
        ...(opts.coverageMaxFiles !== undefined ? { maxFiles: opts.coverageMaxFiles } : {}),
        ...(opts.coverageMaxHotspots !== undefined
          ? { maxHotspots: opts.coverageMaxHotspots }
          : {}),
        ...(opts.coveragePageFit !== undefined ? { pageFit: opts.coveragePageFit } : {}),
      });
    } else {
      const limit = opts.coverageDetail === 'all' ? 'all' : (opts.coverageDetail as number);
      await printDetailedCoverage({
        map: filteredMap,
        root: repoRoot,
        limitPerFile: limit,
        showCode: opts.coverageShowCode ?? Boolean(process.stdout.isTTY),
      });
    }
  }
};

export const runJestBootstrap = async (bootstrap?: string): Promise<void> => {
  const raw = String(bootstrap ?? '').trim();
  if (!raw) {
    return; // no-op when no bootstrap is provided
  }
  const env = safeEnv(process.env, { NODE_ENV: 'test' }) as unknown as NodeJS.ProcessEnv;
  let code = 0;
  if (/\s/.test(raw)) {
    // Full command line: run via shell
    if (process.platform === 'win32') {
      code = await runExitCode('cmd.exe', ['/d', '/s', '/c', raw], { env });
    } else {
      code = await runExitCode('bash', ['-lc', raw], { env });
    }
  } else {
    // Single token: treat as npm script name
    const npmCmd = process.platform === 'win32' ? 'npm.cmd' : 'npm';
    code = await runExitCode(npmCmd, ['run', '-s', raw], { env });
  }
  if (Number(code) !== 0) {
    throw new Error('Jest DB bootstrap failed');
  }
};

export const program = async (): Promise<void> => {
  registerSignalHandlersOnce();
  const argv = process.argv.slice(2);
  const fileConfig = await loadHeadlampConfig();
  const cfgTokens = ((): string[] => {
    const t: string[] = [];
    const pushIf = (cond: boolean, token: string) => {
      if (cond) {
        t.push(token);
      }
    };
    const pushKV = (flag: string, value: string | number) => {
      t.push(`${flag}=${String(value)}`);
    };
    const cfg = fileConfig as HeadlampConfig;
    if (!cfg || typeof cfg !== 'object') {
      return t;
    }
    // 1) Base defaults (always-on)
    if (cfg.bootstrapCommand) {
      pushKV('--bootstrapCommand', cfg.bootstrapCommand);
    }
    if ((cfg as any).sequential === true) {
      t.push('--sequential');
    }
    if (Array.isArray(cfg.jestArgs) && cfg.jestArgs.length) {
      t.push(...cfg.jestArgs);
    }
    // 2) Coverage-context defaults (apply only when coverage is active)
    const argvHasCoverage = argv.some(
      (tok) => tok === '--coverage' || String(tok).startsWith('--coverage='),
    );
    const coverageAlwaysOn = Boolean((cfg as any).coverage === true);
    const coverageObj =
      (cfg as any).coverage && typeof (cfg as any).coverage === 'object'
        ? ((cfg as any).coverage as Record<string, unknown>)
        : undefined;
    if (coverageAlwaysOn && !argvHasCoverage) {
      t.push('--coverage');
    }
    if (coverageAlwaysOn || argvHasCoverage) {
      const abortOnFailure = coverageObj?.abortOnFailure as boolean | undefined;
      const mode = (coverageObj?.mode as string | undefined) ?? (cfg as any).coverageMode;
      const pageFit = (coverageObj?.pageFit as boolean | undefined) ?? (cfg as any).coveragePageFit;
      if (abortOnFailure !== undefined) {
        pushKV('--coverage.abortOnFailure', abortOnFailure ? 'true' : 'false');
      }
      if (mode) {
        pushKV('--coverage.mode', mode);
      }
      if (pageFit !== undefined) {
        pushKV('--coverage.pageFit', pageFit ? 'true' : 'false');
      }
      // keep existing optional extras
      if ((cfg as any).coverageUi) {
        pushKV('--coverage-ui', (cfg as any).coverageUi);
      }
      if ((cfg as any).editorCmd) {
        pushKV('--coverage.editor', (cfg as any).editorCmd);
      }
      if ((cfg as any).coverageDetail !== undefined) {
        pushKV('--coverage.detail', (cfg as any).coverageDetail);
      }
      if ((cfg as any).coverageShowCode !== undefined) {
        pushKV('--coverage.showCode', (cfg as any).coverageShowCode ? 'true' : 'false');
      }
      if ((cfg as any).coverageMaxFiles !== undefined) {
        pushKV('--coverage.maxFiles', (cfg as any).coverageMaxFiles);
      }
      if ((cfg as any).coverageMaxHotspots !== undefined) {
        pushKV('--coverage.maxHotspots', (cfg as any).coverageMaxHotspots);
      }
      if (Array.isArray((cfg as any).include) && (cfg as any).include.length) {
        pushKV('--coverage.include', ((cfg as any).include as string[]).join(','));
      }
      if (Array.isArray((cfg as any).exclude) && (cfg as any).exclude.length) {
        pushKV('--coverage.exclude', ((cfg as any).exclude as string[]).join(','));
      }
    }
    // 3) Changed-context defaults (apply only when changed is active)
    const changedFromCli = ((): string | undefined => {
      for (let i = 0; i < argv.length; i += 1) {
        const tok = String(argv[i] ?? '');
        const nxt = i + 1 < argv.length ? String(argv[i + 1]) : undefined;
        if (tok.startsWith('--changed=')) {
          return tok.split('=')[1] ?? '';
        }
        if (tok === '--changed' && nxt) {
          return nxt;
        }
      }
      return undefined;
    })();
    const changedObj =
      (cfg as any).changed && typeof (cfg as any).changed === 'object'
        ? ((cfg as any).changed as Record<string, any>)
        : (cfg as any).changedSection && typeof (cfg as any).changedSection === 'object'
          ? ((cfg as any).changedSection as Record<string, any>)
          : undefined;
    const changedModeConfig =
      typeof (cfg as any).changed === 'string' ? ((cfg as any).changed as string) : undefined;
    const activeChangedMode = changedFromCli ?? changedModeConfig;
    if (activeChangedMode) {
      const defaultDepth = changedObj?.depth as number | undefined;
      const perMode = changedObj?.[activeChangedMode];
      const overrideDepth =
        perMode && typeof perMode === 'object'
          ? (perMode.depth as number | undefined)
          : (perMode as number | undefined);
      const finalDepth = overrideDepth ?? defaultDepth;
      if (finalDepth !== undefined) {
        pushKV('--changed.depth', finalDepth);
      }
      if (!changedFromCli && changedModeConfig) {
        pushKV('--changed', changedModeConfig);
      }
    }
    return t;
  })();
  const {
    jestArgs,
    collectCoverage,
    coverageUi,
    coverageAbortOnFailure,
    onlyFailures,
    showLogs,
    sequential,
    bootstrapCommand,
    selectionSpecified,
    selectionPaths,
    includeGlobs,
    excludeGlobs,
    editorCmd,
    workspaceRoot,
    coverageDetail,
    coverageShowCode,
    coverageMode,
    coverageMaxFiles: coverageMaxFilesArg,
    coverageMaxHotspots: coverageMaxHotspotsArg,
    coveragePageFit,
    changed,
    changedDepth,
  } = deriveArgs([...cfgTokens, ...argv]);
  // Derive changed-file selection (staged/unstaged/all) when requested
  const getChangedFiles = async (
    mode: 'all' | 'staged' | 'unstaged' | 'branch' | 'lastCommit',
    cwd: string,
  ): Promise<readonly string[]> => {
    const collect = async (cmd: string, args: readonly string[]) => {
      try {
        const out = await runText(cmd, args, {
          cwd,
          env: safeEnv(process.env, {}) as unknown as NodeJS.ProcessEnv,
          timeoutMs: 4000,
        });
        return out
          .split(/\r?\n/)
          .map((line) => line.trim())
          .filter(Boolean);
      } catch {
        return [] as string[];
      }
    };
    if (mode === 'lastCommit') {
      const lastDiff = await collect('git', [
        'diff',
        '--name-only',
        '--diff-filter=ACMRTUXB',
        'HEAD^',
        'HEAD',
      ]);
      const rels = Array.from(new Set(lastDiff));
      return rels
        .map((rel) => path.resolve(cwd, rel).replace(/\\/g, '/'))
        .filter((abs) => !abs.includes('/node_modules/') && !abs.includes('/coverage/'));
    }
    if (mode === 'branch') {
      // Determine default branch (origin/HEAD -> ref or fall back to origin/main, origin/master)
      const resolveDefaultBranch = async (): Promise<string | undefined> => {
        const candidates: string[] = [];
        try {
          const sym = await collect('git', ['symbolic-ref', 'refs/remotes/origin/HEAD']);
          const headRef = sym.find((ln) => ln.includes('refs/remotes/origin/'));
          if (headRef) {
            const m = /refs\/remotes\/(.+)/.exec(headRef);
            if (m && m[1]) {
              candidates.push(m[1]);
            }
          }
        } catch {
          /* ignore */
        }
        candidates.push('origin/main', 'origin/master');
        for (const cand of candidates) {
          // eslint-disable-next-line no-await-in-loop
          const exists = await collect('git', ['rev-parse', '--verify', cand]);
          if (exists.length > 0) {
            return cand;
          }
        }
        return undefined;
      };
      const defaultBranch = await resolveDefaultBranch();
      const mergeBase = defaultBranch
        ? (await collect('git', ['merge-base', 'HEAD', defaultBranch]))[0]
        : undefined;
      const diffBase = mergeBase ?? 'HEAD^';
      const branchDiff = await collect('git', [
        'diff',
        '--name-only',
        '--diff-filter=ACMRTUXB',
        diffBase,
        'HEAD',
      ]);
      // On top of branch diff, include current uncommitted (staged/unstaged) and untracked changes
      const stagedNow = await collect('git', [
        'diff',
        '--name-only',
        '--diff-filter=ACMRTUXB',
        '--cached',
      ]);
      const unstagedNow = await collect('git', ['diff', '--name-only', '--diff-filter=ACMRTUXB']);
      const untrackedNow = await collect('git', ['ls-files', '--others', '--exclude-standard']);
      const rels = Array.from(
        new Set([...branchDiff, ...stagedNow, ...unstagedNow, ...untrackedNow]),
      );
      return rels
        .map((rel) => path.resolve(cwd, rel).replace(/\\/g, '/'))
        .filter((abs) => !abs.includes('/node_modules/') && !abs.includes('/coverage/'));
    }
    const staged =
      mode === 'staged' || mode === 'all'
        ? await collect('git', ['diff', '--name-only', '--diff-filter=ACMRTUXB', '--cached'])
        : [];
    const unstagedTracked =
      mode === 'unstaged' || mode === 'all'
        ? await collect('git', ['diff', '--name-only', '--diff-filter=ACMRTUXB'])
        : [];
    const untracked =
      mode === 'unstaged' || mode === 'all'
        ? await collect('git', ['ls-files', '--others', '--exclude-standard'])
        : [];
    const rels = Array.from(new Set([...staged, ...unstagedTracked, ...untracked]));
    return rels
      .map((rel) => path.resolve(cwd, rel).replace(/\\/g, '/'))
      .filter((abs) => !abs.includes('/node_modules/') && !abs.includes('/coverage/'));
  };
  const repoRootForChanged = workspaceRoot ?? (await findRepoRoot());
  const changedSelectionAbs = changed
    ? await getChangedFiles(changed, repoRootForChanged)
    : ([] as readonly string[]);
  const selectionPathsAugmented = changedSelectionAbs.length
    ? Array.from(new Set([...(selectionPaths as readonly string[]), ...changedSelectionAbs]))
    : selectionPaths;
  const selectionSpecifiedAugmented = Boolean(selectionSpecified || changedSelectionAbs.length > 0);
  console.info(
    `Selection → specified=${selectionSpecifiedAugmented} paths=${selectionPathsAugmented.length}`,
  );
  const { jest } = argsForDiscovery(['run'], jestArgs);
  const selectionLooksLikeTest = selectionPathsAugmented.some(
    (pathText) => /\.(test|spec)\.[tj]sx?$/i.test(pathText) || /(^|\/)tests?\//i.test(pathText),
  );
  const selectionLooksLikePath = selectionPathsAugmented.some(
    (pathText) => /[\\/]/.test(pathText) || /\.(m?[tj]sx?)$/i.test(pathText),
  );
  const selectionHasPaths = selectionPathsAugmented.length > 0;
  const repoRootForDiscovery = workspaceRoot ?? (await findRepoRoot());
  // Detect name-pattern-only selection (no explicit file/path selection or changed files)
  const containsNamePatternForDiscovery = jestArgs.some(
    (arg) => arg === '-t' || arg === '--testNamePattern' || /^--testNamePattern=/.test(String(arg)),
  );
  const namePatternOnlyForDiscovery =
    containsNamePatternForDiscovery &&
    !selectionLooksLikePath &&
    !selectionLooksLikeTest &&
    (changedSelectionAbs?.length ?? 0) === 0;

  // Expand production selections from bare filenames or repo-root-relative suffixes
  const expandProductionSelections = async (
    tokens: readonly string[],
    repoRoot: string,
  ): Promise<readonly string[]> => {
    const results = new Set<string>();
    for (const raw of tokens) {
      const token = String(raw).trim();
      if (!token) {
        continue;
      }
      const isAbs = path.isAbsolute(token);
      const looksLikeRelPath = /[\\/]/.test(token);
      let candidateFromRoot: string | undefined;
      if (token.startsWith('/')) {
        candidateFromRoot = path.join(repoRoot, token.slice(1));
      } else if (looksLikeRelPath) {
        candidateFromRoot = path.join(repoRoot, token);
      } else {
        candidateFromRoot = undefined;
      }
      const tryPushIfProd = (absPath: string) => {
        const norm = path.resolve(absPath).replace(/\\/g, '/');
        const isTest = /(^|\/)tests?\//i.test(norm) || /\.(test|spec)\.[tj]sx?$/i.test(norm);
        if (!isTest && fsSync.existsSync(norm)) {
          results.add(norm);
        }
      };
      if (isAbs && fsSync.existsSync(token)) {
        tryPushIfProd(token);
        continue;
      }
      if (candidateFromRoot && fsSync.existsSync(candidateFromRoot)) {
        tryPushIfProd(candidateFromRoot);
        continue;
      }
      // Use ripgrep to find files whose path ends with the token (filename or suffix)
      try {
        // eslint-disable-next-line no-await-in-loop
        const out = await runText('rg', ['--files', '-g', `**/${token}`], {
          cwd: repoRoot,
          env: safeEnv(process.env, {
            CI: '1',
          }) as unknown as NodeJS.ProcessEnv,
          timeoutMs: 4000,
        });
        const matches = out
          .split(/\r?\n/)
          .map((line) => line.trim())
          .filter(Boolean)
          .map((rel) => path.resolve(repoRoot, rel).replace(/\\/g, '/'))
          .filter(
            (abs) =>
              !abs.includes('/node_modules/') &&
              !abs.includes('/coverage/') &&
              !/(^|\/)tests?\//i.test(abs) &&
              !/\.(test|spec)\.[tj]sx?$/i.test(abs),
          );
        matches.forEach((abs) => results.add(abs));
      } catch {
        // ignore
      }
    }
    return Array.from(results);
  };

  const initialProdSelections = selectionPathsAugmented.filter(
    (pathText) =>
      (/[\\/]/.test(pathText) || /\.(m?[tj]sx?)$/i.test(pathText)) &&
      !/(^|\/)tests?\//i.test(pathText) &&
      !/\.(test|spec)\.[tj]sx?$/i.test(pathText),
  );
  const expandedProdSelections = initialProdSelections.length
    ? initialProdSelections
    : await expandProductionSelections(selectionPathsAugmented, repoRootForDiscovery);
  const selectionIncludesProdPaths = expandedProdSelections.length > 0;
  console.info(
    `Selection classify → looksLikePath=${selectionLooksLikePath} looksLikeTest=${selectionLooksLikeTest} prodPaths=${selectionIncludesProdPaths}`,
  );
  const stripPathTokens = (args: readonly string[]) =>
    args.filter((token) => !selectionPathsAugmented.includes(token));
  const jestDiscoveryArgs = selectionIncludesProdPaths ? stripPathTokens(jest) : jest;

  const projectConfigs: string[] = [];
  try {
    const baseCfg = path.resolve('jest.config.js');
    const tsCfg = path.resolve('jest.ts.config.js');
    if (fsSync.existsSync(baseCfg)) {
      projectConfigs.push(baseCfg);
    }
    if (fsSync.existsSync(tsCfg)) {
      projectConfigs.push(tsCfg);
    }
  } catch (err) {
    console.warn(`Failed to discover Jest project configs: ${String(err)}`);
  }

  const perProjectFiles = new Map<string, string[]>();
  if (!namePatternOnlyForDiscovery && selectionIncludesProdPaths) {
    console.info(
      `Discovering (rg-first) → related=${selectionIncludesProdPaths} | cwd=${repoRootForDiscovery}`,
    );
    const prodSelections = expandedProdSelections;
    await Promise.all(
      projectConfigs.map(async (cfg) => {
        const cfgCwd = path.dirname(cfg);
        const allTests = await discoverJestResilient([...jestDiscoveryArgs, '--config', cfg], {
          cwd: cfgCwd,
        });
        let directPerProject: readonly string[] = [];
        try {
          directPerProject = await selectDirectTestsForProduction({
            rootDir: repoRootForDiscovery,
            testFiles: allTests,
            productionFiles: prodSelections,
          });
        } catch (err) {
          if (isDebug()) {
            console.warn(
              `direct selection failed for project ${path.basename(cfg)}: ${String(err)}`,
            );
          }
        }
        perProjectFiles.set(cfg, directPerProject as string[]);
      }),
    );
  } else if (!namePatternOnlyForDiscovery) {
    console.info(
      `Discovering → jestArgs=${jestDiscoveryArgs.join(
        ' ',
      )} | related=${selectionIncludesProdPaths} | cwd=${repoRootForDiscovery}`,
    );
    await Promise.all(
      projectConfigs.map(async (cfg) => {
        const cfgCwd = path.dirname(cfg);
        const files = await discoverJestCached([...jestDiscoveryArgs, '--config', cfg], {
          cwd: cfgCwd,
        });
        perProjectFiles.set(cfg, files as string[]);
      }),
    );
  }

  // Name-pattern-only: preselect candidate test files by ripgrep-ing for the pattern
  if (namePatternOnlyForDiscovery) {
    const extractTestNamePattern = (args: readonly string[]): string | undefined => {
      for (let i = 0; i < args.length; i += 1) {
        const token = String(args[i] ?? '');
        if (token === '-t' && i + 1 < args.length) {
          return String(args[i + 1]);
        }
        if (token === '--testNamePattern' && i + 1 < args.length) {
          return String(args[i + 1]);
        }
        if (token.startsWith('--testNamePattern=')) {
          return token.split('=')[1] ?? '';
        }
      }
      return undefined;
    };
    const pattern = extractTestNamePattern(jestArgs);
    const repoRoot = repoRootForDiscovery;
    const rgCandidates = async (): Promise<readonly string[]> => {
      if (!pattern || !pattern.trim()) {
        return [] as const;
      }
      const args: string[] = [
        '--no-messages',
        '--line-number',
        '--color',
        'never',
        '--files-with-matches',
        '-e',
        pattern,
        '-g',
        '**/*.test.*',
        '-g',
        '**/*.spec.*',
        '-g',
        'tests/**/*',
        '-g',
        '!**/node_modules/**',
        '-g',
        '!**/coverage/**',
        '-g',
        '!**/dist/**',
        '-g',
        '!**/build/**',
      ];
      try {
        const out = await runText('rg', args, {
          cwd: repoRoot,
          env: safeEnv(process.env, {}) as unknown as NodeJS.ProcessEnv,
          timeoutMs: 4000,
        });
        return out
          .split(/\r?\n/)
          .map((line) => line.trim())
          .filter(Boolean)
          .map((rel) => path.resolve(repoRoot, rel).replace(/\\/g, '/'));
      } catch {
        return [] as const;
      }
    };
    const matched = await rgCandidates();
    if (matched.length > 0) {
      const jestArgsForOwnership = jestDiscoveryArgs;
      for (const cfg of projectConfigs) {
        const cfgCwd = path.dirname(cfg);
        // eslint-disable-next-line no-await-in-loop
        const owned = await filterCandidatesForProject(cfg, jestArgsForOwnership, matched, cfgCwd);
        perProjectFiles.set(cfg, owned as string[]);
      }
    }
  }

  const perProjectFiltered = new Map<string, string[]>();
  for (const cfg of projectConfigs) {
    const files = perProjectFiles.get(cfg) ?? [];
    const selectionTestPaths = selectionPathsAugmented.filter(
      (pathToken) =>
        /\.(test|spec)\.[tj]sx?$/i.test(pathToken) || /(^|\/)tests?\//i.test(pathToken),
    );
    const candidates = selectionHasPaths && selectionLooksLikeTest ? selectionTestPaths : files;
    const absFiles = candidates
      .map((candidatePath) =>
        path.isAbsolute(candidatePath)
          ? candidatePath
          : path.join(repoRootForDiscovery, candidatePath),
      )
      .map((absolutePath) => absolutePath.replace(/\\/g, '/'));
    // eslint-disable-next-line no-await-in-loop
    const onlyOwned = await filterCandidatesForProject(
      cfg,
      jestDiscoveryArgs,
      absFiles,
      path.dirname(cfg),
    );
    perProjectFiltered.set(cfg, onlyOwned as string[]);
  }

  let jestFiles = namePatternOnlyForDiscovery
    ? ([] as string[])
    : Array.from(perProjectFiltered.values()).flat();
  if (!namePatternOnlyForDiscovery) {
    console.info(
      `Discovery results → jest=${jestFiles.length} (projects=${projectConfigs.length})`,
    );
  } else {
    console.info('Discovery skipped (name pattern only).');
  }

  const looksLikeTestPath = (candidatePath: string) =>
    /\.(test|spec)\.[tj]sx?$/i.test(candidatePath) || /(^|\/)tests?\//i.test(candidatePath);
  const prodSelections = expandedProdSelections;

  let effectiveJestFiles = jestFiles.slice();
  if (selectionHasPaths && prodSelections.length > 0) {
    console.info(`rg related → prodSelections=${prodSelections.length} (starting)`);
    const repoRootForRefinement = workspaceRoot ?? (await findRepoRoot());
    const selectionKey = prodSelections
      .map((absPath) => path.relative(repoRootForRefinement, absPath).replace(/\\/g, '/'))
      .sort((firstPath, secondPath) => firstPath.localeCompare(secondPath))
      .join('|');
    const { cachedRelated, findRelatedTestsFast, DEFAULT_TEST_GLOBS } = await import(
      './fast-related'
    );
    const { DEFAULT_EXCLUDE } = await import('./args');
    const rgMatches = await cachedRelated({
      repoRoot: repoRootForRefinement,
      selectionKey,
      compute: () =>
        findRelatedTestsFast({
          repoRoot: repoRootForRefinement,
          productionPaths: prodSelections,
          testGlobs: DEFAULT_TEST_GLOBS,
          excludeGlobs: DEFAULT_EXCLUDE,
          timeoutMs: 1500,
        }),
    });
    console.info(`rg candidates → count=${rgMatches.length}`);
    console.info('rg candidates →');
    const normalizedCandidates = rgMatches.map((candidatePath) =>
      candidatePath.replace(/\\/g, '/'),
    );
    normalizedCandidates.forEach((candidatePath) => console.info(` - ${candidatePath}`));
    const rgSet = new Set(rgMatches.map((candidate) => candidate.replace(/\\/g, '/')));
    if (rgSet.size > 0) {
      if (selectionIncludesProdPaths) {
        // Overwrite jestFiles with rg candidates and re-filter per project ownership
        const rgCandidates = Array.from(rgSet);
        const perProjectFromRg = new Map<string, string[]>();
        for (const cfg of projectConfigs) {
          // eslint-disable-next-line no-await-in-loop
          const owned = await filterCandidatesForProject(
            cfg,
            jestDiscoveryArgs,
            rgCandidates,
            path.dirname(cfg),
          );
          perProjectFromRg.set(cfg, owned as string[]);
        }
        // If ownership filtering lost all candidates,
        // run a content-based import scan on rg candidates to keep only relevant tests
        let totalOwned = Array.from(perProjectFromRg.values()).flat().length;
        if (totalOwned > 0) {
          perProjectFiltered.clear();
          for (const [cfg2, owned] of perProjectFromRg.entries()) {
            perProjectFiltered.set(cfg2, owned);
          }
          jestFiles = Array.from(perProjectFiltered.values()).flat();
          effectiveJestFiles = jestFiles.slice();
        } else {
          const repoRootForScan = repoRootForDiscovery;
          const toSeeds = (abs: string) => {
            const rel = path.relative(repoRootForScan, abs).replace(/\\/g, '/');
            const withoutExt = rel.replace(/\.(m?[tj]sx?)$/i, '');
            const base = path.basename(withoutExt);
            const segs = withoutExt.split('/');
            const tail2 = segs.slice(-2).join('/');
            return Array.from(new Set([withoutExt, base, tail2].filter(Boolean)));
          };
          const seeds = Array.from(new Set(prodSelections.flatMap(toSeeds)));
          const includesSeed = (text: string) => seeds.some((seed) => text.includes(seed));
          const tryRead = (filePath: string): string => {
            try {
              return fsSync.readFileSync(filePath, 'utf8');
            } catch {
              return '';
            }
          };
          const resolveLocalImport = (fromFile: string, spec: string): string | undefined => {
            const baseDir = path.dirname(fromFile);
            const cand = path.resolve(baseDir, spec);
            const exts = ['', '.ts', '.tsx', '.js', '.jsx', '.mjs', '.cjs'];
            for (const ext of exts) {
              const full = ext ? `${cand}${ext}` : cand;
              if (fsSync.existsSync(full)) {
                return full;
              }
            }
            // index files
            for (const ext of exts) {
              const full = path.join(cand, `index${ext}`);
              if (fsSync.existsSync(full)) {
                return full;
              }
            }
            return undefined;
          };
          const importSpecs = (body: string): string[] => {
            const out: string[] = [];
            const importRe = /import\s+[^'"\n]*from\s+['"]([^'"]+)['"];?/g;
            const requireRe = /require\(\s*['"]([^'"]+)['"]\s*\)/g;
            let importMatch: RegExpExecArray | null;
            // eslint-disable-next-line no-cond-assign
            while ((importMatch = importRe.exec(body))) {
              out.push(importMatch[1]!);
            }
            // eslint-disable-next-line no-cond-assign
            let requireMatch: RegExpExecArray | null;
            // eslint-disable-next-line no-cond-assign
            while ((requireMatch = requireRe.exec(body))) {
              out.push(requireMatch[1]!);
            }
            return out;
          };
          const keptCandidates: string[] = [];
          for (const cand of rgCandidates) {
            const body = tryRead(cand);
            if (includesSeed(body)) {
              keptCandidates.push(cand);
              continue;
            }
            const specs = importSpecs(body).filter(
              (sp) => sp.startsWith('.') || sp.startsWith('/'),
            );
            let kept = false;
            for (const spec of specs) {
              const target = resolveLocalImport(cand, spec);
              if (!target) {
                continue;
              }
              const tb = tryRead(target);
              if (includesSeed(tb)) {
                kept = true;
                break;
              }
            }
            if (kept) {
              keptCandidates.push(cand);
            }
          }
          if (keptCandidates.length > 0) {
            const perProjectFromScan = new Map<string, string[]>();
            for (const cfg of projectConfigs) {
              // eslint-disable-next-line no-await-in-loop
              const owned = await filterCandidatesForProject(
                cfg,
                jestDiscoveryArgs,
                keptCandidates,
                path.dirname(cfg),
              );
              perProjectFromScan.set(cfg, owned as string[]);
            }
            totalOwned = Array.from(perProjectFromScan.values()).flat().length;
            if (totalOwned > 0) {
              perProjectFiltered.clear();
              for (const [cfg, owned] of perProjectFromScan.entries()) {
                perProjectFiltered.set(cfg, owned);
              }
              jestFiles = Array.from(perProjectFiltered.values()).flat();
              effectiveJestFiles = jestFiles.slice();
            }
          }
        }
        // If still zero after scan, leave as zero to trigger jest-list fallback later
        // and do NOT assign all candidates to an arbitrary project.
      } else {
        const narrowedJest = effectiveJestFiles.filter((candidate) =>
          rgSet.has(candidate.replace(/\\/g, '/')),
        );
        if (narrowedJest.length > 0) {
          effectiveJestFiles = narrowedJest;
        }
      }
    }
    if (effectiveJestFiles.length === 0) {
      const repoRoot = repoRootForRefinement;
      // If no candidates remain, expand search
      // universe to all discovered Jest tests across projects
      if (jestFiles.length === 0) {
        try {
          const allAcross: string[] = [];
          for (const cfg of projectConfigs) {
            const cfgCwd = path.dirname(cfg);
            // eslint-disable-next-line no-await-in-loop
            const listed = await discoverJestResilient([...jestDiscoveryArgs, '--config', cfg], {
              cwd: cfgCwd,
              // eslint-disable-next-line max-lines
            });
            allAcross.push(...listed);
          }
          const uniqAll = Array.from(new Set(allAcross.map((p) => p.replace(/\\/g, '/'))));
          if (uniqAll.length > 0) {
            jestFiles = uniqAll;
          }
        } catch {
          // ignore
        }
      }
      const seeds = prodSelections
        .map((abs) =>
          path
            .relative(repoRoot, abs)
            .replace(/\\/g, '/')
            .replace(/\.(m?[tj]sx?)$/i, ''),
        )
        .flatMap((rel) => {
          const base = path.basename(rel);
          const segments = rel.split('/');
          return Array.from(new Set([rel, base, segments.slice(-2).join('/')].filter(Boolean)));
        });

      const includesSeed = (text: string) => seeds.some((seed) => text.includes(seed));
      const tryReadFile = async (absPath: string): Promise<string> => {
        try {
          return await fs.readFile(absPath, 'utf8');
        } catch {
          return '';
        }
      };
      const resolveLocalImport = (fromFile: string, spec: string): string | undefined => {
        const baseDir = path.dirname(fromFile);
        const candidate = path.resolve(baseDir, spec);
        const extensions = ['', '.ts', '.tsx', '.js', '.jsx', '.mjs', '.cjs', '.mts', '.cts'];
        for (const ext of extensions) {
          const fullPath = ext ? `${candidate}${ext}` : candidate;
          if (fsSync.existsSync(fullPath)) {
            return fullPath;
          }
        }
        for (const ext of extensions) {
          const fullPath = path.join(candidate, `index${ext}`);
          if (fsSync.existsSync(fullPath)) {
            return fullPath;
          }
        }
        return undefined;
      };
      const importOrExportSpecs = (body: string): string[] => {
        const results: string[] = [];
        const importRegex = /import\s+[^'"\n]*from\s+['"]([^'"]+)['"];?/g;
        const requireRegex = /require\(\s*['"]([^'"]+)['"]\s*\)/g;
        const exportFromRegex = /export\s+(?:\*|\{[^}]*\})\s*from\s*['"]([^'"]+)['"];?/g;
        let match: RegExpExecArray | null;
        // eslint-disable-next-line no-cond-assign
        while ((match = importRegex.exec(body))) {
          results.push(match[1]!);
        }
        // eslint-disable-next-line no-cond-assign
        while ((match = requireRegex.exec(body))) {
          results.push(match[1]!);
        }
        // eslint-disable-next-line no-cond-assign
        while ((match = exportFromRegex.exec(body))) {
          results.push(match[1]!);
        }
        return results;
      };

      const union = Array.from(new Set<string>(jestFiles));
      const keep = new Set<string>();
      const visitedBodyCache = new Map<string, string>();
      const specCache = new Map<string, readonly string[]>();
      const resolutionCache = new Map<string, string | undefined>();

      const getBody = async (absPath: string): Promise<string> => {
        const existing = visitedBodyCache.get(absPath);
        if (existing !== undefined) {
          return existing;
        }
        const content = await tryReadFile(absPath);
        visitedBodyCache.set(absPath, content);
        return content;
      };

      const getSpecs = async (absPath: string): Promise<readonly string[]> => {
        const cached = specCache.get(absPath);
        if (cached !== undefined) {
          return cached;
        }
        const body = await getBody(absPath);
        const specs = importOrExportSpecs(body);
        specCache.set(absPath, specs);
        return specs;
      };

      const resolveSpec = (fromFile: string, spec: string): string | undefined => {
        const key = `${fromFile}|${spec}`;
        if (resolutionCache.has(key)) {
          return resolutionCache.get(key);
        }
        const resolved =
          spec.startsWith('.') || spec.startsWith('/')
            ? resolveLocalImport(fromFile, spec)
            : undefined;
        resolutionCache.set(key, resolved);
        return resolved;
      };

      const MAX_DEPTH =
        Number.isFinite(Number(changedDepth)) && Number(changedDepth) > 0
          ? Number(changedDepth)
          : 5;
      const seen = new Set<string>();
      const matchesTransitively = async (absTestPath: string, depth: number): Promise<boolean> => {
        if (depth > MAX_DEPTH) {
          return false;
        }
        const cacheKey = `${absTestPath}@${depth}`;
        if (seen.has(cacheKey)) {
          return false;
        }
        seen.add(cacheKey);
        const body = await getBody(absTestPath);
        if (includesSeed(body)) {
          return true;
        }
        const specs = await getSpecs(absTestPath);
        for (const spec of specs) {
          const target = resolveSpec(absTestPath, spec);
          if (!target) {
            continue;
          }
          // eslint-disable-next-line no-await-in-loop
          const targetBody = await getBody(target);
          if (includesSeed(targetBody)) {
            return true;
          }
          // eslint-disable-next-line no-await-in-loop
          if (await matchesTransitively(target, depth + 1)) {
            return true;
          }
        }
        return false;
      };

      const concurrency = 16;
      let scanIndex = 0;
      const workers: Promise<void>[] = [];
      for (let workerIndex = 0; workerIndex < concurrency; workerIndex += 1) {
        workers.push(
          // eslint-disable-next-line no-loop-func
          (async () => {
            // eslint-disable-next-line no-constant-condition
            while (true) {
              const currentIndex = scanIndex;
              if (currentIndex >= union.length) {
                break;
              }
              scanIndex += 1;
              const candidate = union[currentIndex]!;
              // eslint-disable-next-line no-await-in-loop
              const ok = await matchesTransitively(candidate, 0);
              if (ok) {
                keep.add(candidate);
              }
            }
          })(),
        );
      }
      await Promise.all(workers);

      const jestKept = jestFiles
        .filter((candidate) => keep.has(candidate))
        .sort((left, right) => left.localeCompare(right));
      if (jestKept.length) {
        effectiveJestFiles = jestKept;
      }
      console.info(`fallback refine (transitive) → jest=${effectiveJestFiles.length}`);
    }
  }

  const jestDecision = decideShouldRunJest([], effectiveJestFiles, {
    selectionSpecified: selectionSpecifiedAugmented,
    selectionPaths: selectionPathsAugmented,
  });
  const forcedByNamePattern = namePatternOnlyForDiscovery;
  const shouldRunJest = forcedByNamePattern ? true : jestDecision.shouldRunJest;
  const jestCount = effectiveJestFiles.length;
  const sharePct = Math.round((forcedByNamePattern ? 1 : jestDecision.share) * 100);
  const msg = shouldRunJest
    ? `Jest selected (${sharePct}% of discovered tests; reason: ${forcedByNamePattern ? 'name_pattern' : jestDecision.reason})`
    : `Skipping Jest (${sharePct}% of discovered tests; reason: ${jestDecision.reason})`;
  console.info(`Discovery → jest: ${jestCount}. ${msg}`);

  if (!shouldRunJest) {
    console.warn('No matching tests were discovered for either Vitest or Jest.');
    console.info(`Jest args: ${[...jestDiscoveryArgs, '--listTests'].join(' ')}`);
    console.info(
      'Tip: check your -t/--testNamePattern and file path; Jest lists files selected by your patterns.',
    );
    process.exit(1);
    return;
  }

  console.info(`Run plan → Jest maybe=${shouldRunJest} (projects=${projectConfigs.length})`);
  let jestExitCode = 0;
  const allBridgeJson: Array<ReturnType<typeof coerceJestJsonToBridge>> = [];
  const executedTestFilesSet = new Set<string>();
  if (shouldRunJest) {
    console.info('Starting Jest (no Vitest targets)…');
    await runJestBootstrap(bootstrapCommand);
    const jestRunArgs = selectionIncludesProdPaths ? stripPathTokens(jestArgs) : jestArgs;
    const sanitizedJestRunArgs = jestRunArgs.filter(
      (arg) => !/^--coverageDirectory(?:=|$)/.test(String(arg)),
    );
    const projectsToRun = namePatternOnlyForDiscovery
      ? projectConfigs
      : projectConfigs.filter((cfg) => (perProjectFiltered.get(cfg) ?? []).length > 0);
    const stripFooter = (text: string): string => {
      const lines = text.split('\n');
      const idx = lines.findIndex((ln) => /^Test Files\s/.test(stripAnsiSimple(ln)));
      return idx >= 0 ? lines.slice(0, idx).join('\n').trimEnd() : text;
    };
    // Compute directness order for the whole run (project-agnostic list)
    const prodSeedsForRun = ((): readonly string[] => {
      const changedAbs = (changedSelectionAbs ?? []).map((absPath) =>
        path.resolve(absPath).replace(/\\/g, '/'),
      );
      const selAbs = (selectionPathsAugmented as readonly string[]).map((pathToken) =>
        path.resolve(pathToken).replace(/\\/g, '/'),
      );
      return (changedAbs.length ? changedAbs : selAbs).filter(
        (abs) =>
          /[\\/]/.test(abs) &&
          !/(^|\/)tests?\//i.test(abs) &&
          !/\.(test|spec)\.[tj]sx?$/i.test(abs),
      );
    })();
    const repoRootForRank = repoRootForDiscovery;
    const fileRank = await computeDirectnessRank({
      repoRoot: repoRootForRank,
      productionSeeds: prodSeedsForRun,
    });

    const runOneProject = async (cfg: string): Promise<void> => {
      const files = perProjectFiltered.get(cfg) ?? [];
      if (files.length === 0) {
        console.info(`Project ${path.basename(cfg)}: 0 matching tests after filter; skipping.`);
        return;
      }
      files.forEach((absTestPath) =>
        executedTestFilesSet.add(path.resolve(absTestPath).replace(/\\/g, '/')),
      );
      const outJson = path.join(
        os.tmpdir(),
        `jest-bridge-${Date.now()}-${Math.random().toString(36).slice(2)}.json`,
      );
      const reporterPath = path.resolve('scripts/jest-vitest-bridge.cjs');
      try {
        const needsWrite = (() => {
          try {
            const existing = fsSync.readFileSync(reporterPath, 'utf8');
            return existing !== JEST_BRIDGE_REPORTER_SOURCE;
          } catch {
            return true;
          }
        })();
        if (needsWrite) {
          fsSync.mkdirSync(path.dirname(reporterPath), { recursive: true });
          fsSync.writeFileSync(reporterPath, JEST_BRIDGE_REPORTER_SOURCE, 'utf8');
        }
        // ensure environment file exists
        const envPath = path.resolve('scripts/jest-bridge-env.cjs');
        try {
          const existingEnv = fsSync.readFileSync(envPath, 'utf8');
          if (existingEnv !== JEST_BRIDGE_ENV_SOURCE) {
            fsSync.writeFileSync(envPath, JEST_BRIDGE_ENV_SOURCE, 'utf8');
          }
        } catch {
          try {
            fsSync.mkdirSync(path.dirname(envPath), { recursive: true });
          } catch {}
          fsSync.writeFileSync(envPath, JEST_BRIDGE_ENV_SOURCE, 'utf8');
        }
      } catch (ensureReporterError) {
        console.warn(`Unable to ensure jest bridge reporter: ${String(ensureReporterError)}`);
      }
      // eslint-disable-next-line no-await-in-loop
      // Ensure any explicitly selected paths (tests or production files) are included in coverage
      const selectedFilesForCoverage = selectionPathsAugmented
        .filter((pathToken) => /[\\/]/.test(pathToken))
        // Avoid restricting coverage to test files when a test path is selected
        .filter((pathToken) => !looksLikeTestPath(pathToken))
        .map((pathToken) => path.relative(repoRootForDiscovery, pathToken).replace(/\\\\/g, '/'))
        .filter((rel) => rel && !/^\.+\//.test(rel))
        .map((rel) => (rel.startsWith('./') ? rel : `./${rel}`));
      const coverageFromArgs: string[] = [];
      for (const relPath of selectedFilesForCoverage) {
        coverageFromArgs.push('--collectCoverageFrom', relPath);
      }

      const runArgs = [
        '--config',
        cfg,
        '--testLocationInResults',
        ...(namePatternOnlyForDiscovery ? [] : ['--runTestsByPath']),
        `--reporters=${reporterPath}`,
        '--colors',
        ...sanitizedJestRunArgs,
        ...(collectCoverage
          ? [
              '--coverageDirectory',
              path.join('coverage', 'jest', path.basename(cfg).replace(/[^a-zA-Z0-9_.-]+/g, '_')),
            ]
          : []),
        ...coverageFromArgs,
        ...(showLogs ? ['--no-silent'] : []),
        '--passWithNoTests',
        '--verbose',
        ...(namePatternOnlyForDiscovery ? [] : files),
      ];
      if (isDebug() || showLogs) {
        const hasSilentFalse = runArgs.includes('--silent=false');
        const hasEnv = runArgs.includes('--testEnvironment');
        console.info(
          `debug: showLogs=${String(showLogs)} hasSilentFalse=${String(hasSilentFalse)} hasTestEnvironment=${String(hasEnv)}`,
        );
      }
      const { code, output } = await runWithCapture(jestBin, runArgs, {
        env: safeEnv(process.env, {
          NODE_ENV: 'test',
          JEST_BRIDGE_OUT: outJson,
          JEST_BRIDGE_DEBUG: showLogs ? '1' : undefined,
          JEST_BRIDGE_DEBUG_PATH: showLogs
            ? path.resolve(os.tmpdir(), `jest-bridge-debug-${Date.now()}.log`)
            : undefined,
          FORCE_COLOR: '3',
          TERM: process.env.TERM || 'xterm-256color',
        }) as unknown as NodeJS.ProcessEnv,
      });
      let pretty = '';
      try {
        const debug = isDebug();
        if (debug) {
          const capturedLen = output.length;
          console.info(`jest captured output length=${capturedLen}`);
          const fileSizeBytes = fsSync.existsSync(outJson) ? fsSync.statSync(outJson).size : -1;
          console.info(`bridge json @ ${outJson} size=${fileSizeBytes}`);
        }
        const jsonText = fsSync.readFileSync(outJson, 'utf8');
        const parsed = JSON.parse(jsonText) as unknown;
        const bridgeBase = coerceJestJsonToBridge(parsed);
        const filteredForNamePattern = (() => {
          if (!namePatternOnlyForDiscovery) {
            return bridgeBase;
          }
          const keptFiles = bridgeBase.testResults
            .map((file) => ({
              ...file,
              testResults: file.testResults.filter(
                (t) => t.status === 'passed' || t.status === 'failed',
              ),
            }))
            .filter((file) => file.testResults.length > 0);
          const numFailedTests = keptFiles
            .flatMap((f) => f.testResults)
            .filter((t) => t.status === 'failed').length;
          const numPassedTests = keptFiles
            .flatMap((f) => f.testResults)
            .filter((t) => t.status === 'passed').length;
          const numTotalTests = numFailedTests + numPassedTests;
          const numFailedSuites = keptFiles.filter((f) =>
            f.testResults.some((t) => t.status === 'failed'),
          ).length;
          const numPassedSuites = keptFiles.length - numFailedSuites;
          return {
            ...bridgeBase,
            testResults: keptFiles,
            aggregated: {
              ...bridgeBase.aggregated,
              numTotalTestSuites: keptFiles.length,
              numPassedTestSuites: numPassedSuites,
              numFailedTestSuites: numFailedSuites,
              numTotalTests,
              numPassedTests,
              numFailedTests,
              numPendingTests: 0,
              numTodoTests: 0,
              success: numFailedTests === 0,
            },
          } as typeof bridgeBase;
        })();
        // Parse bridge events and summarize for debugging
        const consoleByFile = (() => {
          const by = new Map<string, Array<{ type?: string; message?: string; origin?: string }>>();
          try {
            const lines = output.split(/\r?\n/);
            let totalEvents = 0;
            let envReadyCount = 0;
            let consoleCount = 0;
            let consoleBatchCount = 0;
            let httpCount = 0;
            let assertionCount = 0;
            for (const line of lines) {
              const idx = line.indexOf('[JEST-BRIDGE-EVENT]');
              if (idx < 0) {
                continue;
              } // eslint-disable-line no-continue
              const payload = line.slice(idx + '[JEST-BRIDGE-EVENT]'.length).trim();
              if (!payload) {
                continue;
              } // eslint-disable-line no-continue
              let obj: any;
              try {
                obj = JSON.parse(payload);
              } catch {
                obj = null;
              }
              if (!obj || !obj.type) {
                continue;
              } // eslint-disable-line no-continue
              const testPath =
                typeof obj.testPath === 'string' ? obj.testPath.replace(/\\/g, '/') : undefined;
              totalEvents += 1;
              if (obj.type === 'envReady') {
                envReadyCount += 1;
              }
              if (obj.type === 'console') {
                consoleCount += 1;
              }
              if (obj.type === 'consoleBatch') {
                consoleBatchCount += 1;
              }
              if (
                obj.type === 'httpResponse' ||
                obj.type === 'httpAbort' ||
                obj.type === 'httpResponseBatch'
              ) {
                httpCount += 1;
              }
              if (obj.type === 'assertionFailure') {
                assertionCount += 1;
              }
              if (!testPath) {
                continue;
              } // eslint-disable-line no-continue
              if (obj.type === 'console') {
                const arr = by.get(testPath) || [];
                arr.push({ type: obj.level || 'log', message: obj.message || '' });
                by.set(testPath, arr);
              } else if (obj.type === 'consoleBatch' && Array.isArray(obj.entries)) {
                const arr = by.get(testPath) || [];
                for (const e of obj.entries) {
                  arr.push({ type: (e && e.type) || 'log', message: (e && e.message) || '' });
                }
                by.set(testPath, arr);
              }
            }
            if (isDebug() || showLogs) {
              console.info(
                `debug: bridge events total=${totalEvents} envReady=${envReadyCount} console=${consoleCount} consoleBatch=${consoleBatchCount} http=${httpCount} assertion=${assertionCount}`,
              );
              // process.exit(1);
            }
          } catch {
            /* ignore */
          }
          return by;
        })();
        const bridge = (() => {
          if (consoleByFile.size === 0) {
            return filteredForNamePattern;
          }
          const files = filteredForNamePattern.testResults.map((fileResult) => {
            const key = String(fileResult.testFilePath || '').replace(/\\/g, '/');
            const extraEntries = consoleByFile.get(key) || [];
            if (!extraEntries.length) {
              return fileResult;
            }
            const mergedConsole = [
              ...((fileResult as any).console || []),
              ...extraEntries.map((entry) => ({
                message: entry.message,
                type: entry.type,
                origin: entry.origin,
              })),
            ];
            return { ...(fileResult as any), console: mergedConsole } as typeof fileResult;
          });
          if (isDebug() || showLogs) {
            const sample = files
              .map((f) => ({
                file: String((f as any).testFilePath || '')
                  .split('/')
                  .slice(-2)
                  .join('/'),
                consoleCount: Array.isArray((f as any).console) ? (f as any).console.length : 0,
              }))
              .slice(0, 5);
            console.info(`debug: per-file console counts (first 5): ${JSON.stringify(sample)}`);
          }
          return { ...filteredForNamePattern, testResults: files } as typeof filteredForNamePattern;
        })();
        allBridgeJson.push(bridge);
        // Reorder per-file results by directness and failure before rendering
        try {
          const reordered = {
            ...bridge,
            testResults: sortTestResultsWithRank(fileRank, bridge.testResults).reverse(),
          } as typeof bridge;
          pretty = renderVitestFromJestJSON(
            reordered,
            makeCtx(
              { cwd: repoRootForDiscovery, ...(editorCmd !== undefined ? { editorCmd } : {}) },
              /\bFAIL\b/.test(stripAnsiSimple(output)),
              Boolean(showLogs),
            ),
            { onlyFailures },
          );
        } catch {
          pretty = renderVitestFromJestJSON(
            bridge,
            makeCtx(
              { cwd: repoRootForDiscovery, ...(editorCmd !== undefined ? { editorCmd } : {}) },
              /\bFAIL\b/.test(stripAnsiSimple(output)),
              Boolean(showLogs),
            ),
            { onlyFailures },
          );
        }
        if (debug) {
          const preview = pretty.split('\n').slice(0, 3).join('\n');
          console.info(`pretty preview (json):\n${preview}${pretty.includes('\n') ? '\n…' : ''}`);
        }
      } catch (jsonErr) {
        const debug = isDebug();
        if (debug) {
          console.info('renderer: fallback to text prettifier (missing/invalid JSON)');
          console.info(String(jsonErr));
          console.info(`fallback: raw output lines=${output.split(/\r?\n/).length}`);
        }
        pretty = formatJestOutputVitest(output, {
          cwd: repoRootForDiscovery,
          ...(editorCmd !== undefined ? { editorCmd } : {}),
          onlyFailures,
          showLogs,
        });
        if (debug) {
          const preview = pretty.split('\n').slice(0, 3).join('\n');
          console.info(`pretty preview (text):\n${preview}${pretty.includes('\n') ? '\n…' : ''}`);
        }
      }
      // If the bridge output still looks sparse (common `Error:` with no detail),
      // append raw text rendering as an extra hint source.
      try {
        const looksSparse =
          /\n\s*Error:\s*\n/.test(pretty) &&
          !/(Message:|Thrown:|Events:|Console errors:)/.test(pretty);
        if (looksSparse) {
          const rawAlso = formatJestOutputVitest(output, {
            cwd: repoRootForDiscovery,
            ...(editorCmd !== undefined ? { editorCmd } : {}),
            onlyFailures,
            showLogs,
          });
          const merged = `${stripFooter(pretty)}\n${stripFooter(rawAlso)}`.trimEnd();
          pretty = merged;
        }
      } catch {
        /* ignore raw merge failures */
      }
      // Always drop per-project footer; we'll print a unified summary later
      pretty = stripFooter(pretty);
      if (pretty.trim().length > 0) {
        process.stdout.write(pretty.endsWith('\n') ? pretty : `${pretty}\n`);
      }
      if (Number(code) !== 0) {
        jestExitCode = code;
      }
    };
    // Run projects concurrently with a fixed stride unless sequential is requested
    const stride = sequential ? 1 : 3;
    await runParallelStride(projectsToRun, stride, async (cfg, index) => {
      await runOneProject(cfg as string);
    });
  } else {
    console.info('Jest run skipped based on selection and thresholds.');
  }

  // Print unified merged summary across all projects when available
  if (allBridgeJson.length > 0) {
    const agg = allBridgeJson.map((bridge) => bridge.aggregated);
    const sum = (select: (arg: (typeof agg)[number]) => number) =>
      agg.reduce((total, item) => total + (select(item) || 0), 0);
    const startTime = Math.min(
      ...allBridgeJson.map((bridge) => Number(bridge.startTime || Date.now())),
    );
    const unified = {
      startTime,
      testResults: allBridgeJson.flatMap((bridge) => bridge.testResults),
      aggregated: {
        numTotalTestSuites: sum((item) => item.numTotalTestSuites),
        numPassedTestSuites: sum((item) => item.numPassedTestSuites),
        numFailedTestSuites: sum((item) => item.numFailedTestSuites),
        numTotalTests: sum((item) => item.numTotalTests),
        numPassedTests: sum((item) => item.numPassedTests),
        numFailedTests: sum((item) => item.numFailedTests),
        numPendingTests: sum((item) => item.numPendingTests),
        numTodoTests: sum((item) => item.numTodoTests),
        numTimedOutTests: sum((item) => Number((item as any).numTimedOutTests ?? 0)),
        numTimedOutTestSuites: sum((item) => Number((item as any).numTimedOutTestSuites ?? 0)),
        startTime,
        success: agg.every((item) => Boolean(item.success)),
        runTimeMs: sum((item) => Number(item.runTimeMs ?? 0)),
      },
    } as const;
    // Order results by directness (import-graph distance)
    try {
      const prodSeeds = ((): readonly string[] => {
        const changedAbs = (changedSelectionAbs ?? []).map((absPath) =>
          path.resolve(absPath).replace(/\\/g, '/'),
        );
        const selAbs = (selectionPathsAugmented as readonly string[]).map((pathToken) =>
          path.resolve(pathToken).replace(/\\/g, '/'),
        );
        return (changedAbs.length ? changedAbs : selAbs).filter(
          (abs) =>
            /[\\/]/.test(abs) &&
            !/(^|\/)tests?\//i.test(abs) &&
            !/\.(test|spec)\.[tj]sx?$/i.test(abs),
        );
      })();
      const unifiedRank = await computeDirectnessRank({
        repoRoot: repoRootForDiscovery,
        productionSeeds: prodSeeds,
      });
      const ordered = sortTestResultsWithRank(unifiedRank, unified.testResults).reverse();
      // eslint-disable-next-line no-param-reassign
      (unified as any).testResults = ordered;
    } catch {
      // ignore relevance sorting on failure
    }
    const showStacks = Boolean((unified as any).aggregated?.numFailedTests > 0);
    let text = renderVitestFromJestJSON(
      unified as unknown as BridgeJSON,
      makeCtx(
        { cwd: repoRootForDiscovery, ...(editorCmd !== undefined ? { editorCmd } : {}) },
        showStacks,
        Boolean(showLogs),
      ),
      { onlyFailures },
    );
    if (onlyFailures) {
      text = text
        .split(/\r?\n/)
        .filter((line) => !/^\s*PASS\b/.test(stripAnsiSimple(line)))
        .join('\n');
    }
    if (text.trim().length > 0) {
      process.stdout.write(text.endsWith('\n') ? text : `${text}\n`);
    }
  }

  const finalExitCode = jestExitCode;
  if (collectCoverage && shouldRunJest && coverageAbortOnFailure && finalExitCode !== 0) {
    process.exit(finalExitCode);
    return;
  }
  // Only compute and print coverage if we are not aborting after failures
  if (collectCoverage && shouldRunJest) {
    await mergeLcov();
    const repoRoot = workspaceRoot ?? (await findRepoRoot());
    const mergedOptsBase = {
      selectionSpecified: selectionSpecifiedAugmented,
      selectionPaths: selectionPathsAugmented,
      includeGlobs,
      excludeGlobs,
      workspaceRoot: repoRoot,
      ...(editorCmd !== undefined ? { editorCmd } : {}),
      ...(coverageDetail !== undefined ? { coverageDetail } : {}),
      ...(coverageShowCode !== undefined ? { coverageShowCode } : {}),
      coverageMode,
      ...(coverageMaxFilesArg !== undefined ? { coverageMaxFiles: coverageMaxFilesArg } : {}),
      ...(coverageMaxHotspotsArg !== undefined
        ? { coverageMaxHotspots: coverageMaxHotspotsArg }
        : {}),
      coveragePageFit,
      executedTests: Array.from(executedTestFilesSet),
    } as const;
    await emitMergedCoverage(coverageUi, mergedOptsBase);
  }
  process.exit(finalExitCode);
};
