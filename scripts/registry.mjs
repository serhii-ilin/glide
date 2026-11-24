#!/usr/bin/env node
// Slug-keyed requirement/feature registry.
//
//   node scripts/registry.mjs                  generate docs/features.md and docs/manual-tests.md
//   node scripts/registry.mjs --check          validate the registry (exit 1 on errors)
//   node scripts/registry.mjs --root <dir>     use <dir> instead of the repository root
//
// Source of truth: docs/features/<slug>.md and docs/requirements/<slug>.md. See docs/registry.md.
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const args = process.argv.slice(2);
const rootIdx = args.indexOf('--root');
const root = rootIdx >= 0
  ? path.resolve(args[rootIdx + 1])
  : path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const checkOnly = args.includes('--check');

const SLUG_RE = /^[a-z0-9]+(-[a-z0-9]+)*$/;
const FEATURE_STATUS = ['planned', 'in-progress', 'done', 'partial', 'deferred', 'removed'];
const REQUIREMENT_STATUS = ['active', 'removed'];

// ---- parsing ---------------------------------------------------------------------------------

// Front matter subset: `key: <JSON>` lines; a bare `key:` takes the following indented lines as
// one JSON value; a value that is not JSON is read as a plain string.
function parseFrontMatter(text, file) {
  const m = text.match(/^---\n([\s\S]*?)\n---\n?([\s\S]*)$/);
  if (!m) throw new Error(`${file}: missing front matter`);
  const data = {};
  const lines = m[1].split('\n');
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    if (!line.trim() || line.trimStart().startsWith('#')) continue;
    const km = line.match(/^([A-Za-z_][\w-]*):\s*(.*)$/);
    if (!km) throw new Error(`${file}: cannot parse front matter line ${JSON.stringify(line)}`);
    let raw = km[2];
    if (raw === '') {
      const block = [];
      while (i + 1 < lines.length && /^\s+/.test(lines[i + 1])) block.push(lines[++i]);
      raw = block.join('\n');
    }
    try { data[km[1]] = JSON.parse(raw); } catch { data[km[1]] = raw; }
  }
  return { data, body: m[2].trim() };
}

function loadDir(kind, dir) {
  const full = path.join(root, dir);
  if (!fs.existsSync(full)) return { items: [], errors: [`${dir}: directory is missing`] };
  const items = [], errors = [];
  for (const name of fs.readdirSync(full).filter((n) => n.endsWith('.md')).sort()) {
    const file = `${dir}/${name}`;
    try {
      const { data, body } = parseFrontMatter(fs.readFileSync(path.join(full, name), 'utf8'), file);
      items.push({ file, name: name.slice(0, -3), data, body, kind });
    } catch (e) {
      errors.push(e.message);
    }
  }
  return { items, errors };
}

// ---- validation ------------------------------------------------------------------------------

function rustFnNames() {
  const names = new Set();
  const walk = (dir) => {
    if (!fs.existsSync(dir)) return;
    for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
      const p = path.join(dir, e.name);
      if (e.isDirectory()) walk(p);
      else if (e.name.endsWith('.rs')) {
        for (const m of fs.readFileSync(p, 'utf8').matchAll(/\bfn\s+([A-Za-z_]\w*)/g)) names.add(m[1]);
      }
    }
  };
  walk(path.join(root, 'src'));
  walk(path.join(root, 'tests'));
  return names;
}

// `a::b::{c,d}` -> [a::b::c, a::b::d]; non-test-looking entries yield nothing.
function testIdentifiers(entry) {
  const text = entry.replace(/`/g, '').trim();
  if (/^(manual|n\/a)/i.test(text) || text.includes('*') || text.includes('/')) return [];
  const brace = text.match(/^(.*?)\{([^}]*)\}$/);
  const list = brace ? brace[2].split(',').map((s) => brace[1] + s.trim()) : [text];
  return list.filter((id) => /^[A-Za-z_]\w*(::\w+)*$/.test(id) && (id.includes('::') || id.includes('_')));
}

function validate({ features, requirements, loadErrors }) {
  const errors = [...loadErrors], warnings = [];
  const all = [...features, ...requirements];
  const isArr = (v) => Array.isArray(v);
  const str = (v) => typeof v === 'string' && v.trim() !== '';
  const bySlug = { feature: new Map(), requirement: new Map() };
  const aliasOwner = new Map();
  const seen = new Set();

  for (const it of all) {
    const d = it.data, where = it.file;
    for (const f of ['slug', 'kind', 'title', 'status']) {
      if (!str(d[f])) errors.push(`${where}: missing required field '${f}'`);
    }
    if (str(d.slug)) {
      if (!SLUG_RE.test(d.slug)) errors.push(`${where}: slug '${d.slug}' is not kebab-case`);
      if (d.slug !== it.name) errors.push(`${where}: slug '${d.slug}' must equal the filename '${it.name}'`);
      const key = `${it.kind}:${d.slug}`;
      if (seen.has(key)) errors.push(`${where}: duplicate slug '${d.slug}'`);
      seen.add(key);
      bySlug[it.kind].set(d.slug, it);
    }
    if (d.kind !== undefined && d.kind !== it.kind) errors.push(`${where}: kind '${d.kind}' does not match its directory (${it.kind})`);
    for (const a of d.aliases === undefined ? [] : isArr(d.aliases) ? d.aliases : (errors.push(`${where}: aliases must be a list`), [])) {
      if (aliasOwner.has(a)) errors.push(`${where}: alias '${a}' is already used by ${aliasOwner.get(a)}`);
      else aliasOwner.set(a, where);
    }
  }

  const allSlugs = new Set([...bySlug.feature.keys(), ...bySlug.requirement.keys()]);
  const KEYS = {
    feature: ['slug', 'kind', 'title', 'status', 'milestone', 'aliases', 'requirements', 'gherkin', 'tests', 'mcp', 'manual'],
    requirement: ['slug', 'kind', 'title', 'status', 'milestone', 'aliases', 'features', 'source'],
  };
  for (const it of all) {
    for (const k of Object.keys(it.data)) {
      if (!KEYS[it.kind].includes(k)) errors.push(`${it.file}: unknown front-matter key '${k}'`);
    }
    for (const a of isArr(it.data.aliases) ? it.data.aliases : []) {
      if (allSlugs.has(a)) errors.push(`${it.file}: alias '${a}' is also a slug`);
    }
  }

  // The same slug in both directories is allowed, but flag a duplicate across kinds as a warning.
  for (const s of bySlug.feature.keys()) {
    if (bySlug.requirement.has(s)) warnings.push(`slug '${s}' exists as both a feature and a requirement`);
  }

  for (const f of features) {
    const d = f.data, where = f.file;
    if (str(d.status) && !FEATURE_STATUS.includes(d.status.split(/[\s(]/)[0])) errors.push(`${where}: unknown status '${d.status}'`);
    if (!isArr(d.requirements)) errors.push(`${where}: 'requirements' must be a list (may be empty)`);
    if (!str(d.mcp)) errors.push(`${where}: missing required field 'mcp'`);
    if (!str(d.gherkin)) errors.push(`${where}: missing required field 'gherkin'`);
    if (!isArr(d.tests)) errors.push(`${where}: 'tests' must be a list`);
    if (!f.body) errors.push(`${where}: empty description body`);
    for (const r of isArr(d.requirements) ? d.requirements : []) {
      const req = bySlug.requirement.get(r);
      if (!req) errors.push(`${where}: unknown requirement '${r}'`);
      else if (!(isArr(req.data.features) && req.data.features.includes(d.slug))) errors.push(`${where}: requirement '${r}' does not list this feature back`);
    }
    if (str(d.gherkin) && /\.feature$/.test(d.gherkin) && !fs.existsSync(path.join(root, d.gherkin))) {
      errors.push(`${where}: gherkin file '${d.gherkin}' does not exist`);
    }
    if (str(d.gherkin)) {
      const placeholder = /^(backfill pending|planned \(spec added when promoted\))$/.test(d.gherkin);
      if (!placeholder && d.gherkin !== `tests/features/${d.slug}.feature`) {
        errors.push(`${where}: gherkin must be 'tests/features/${d.slug}.feature' or a known placeholder, got '${d.gherkin}'`);
      }
    }
    for (const entry of isArr(d.tests) ? d.tests : []) {
      for (const g of String(entry).match(/tests\/features\/[\w./-]+\.feature/g) || []) {
        if (!fs.existsSync(path.join(root, g))) errors.push(`${where}: tests entry references missing file '${g}'`);
      }
      for (const [, slug, n] of String(entry).matchAll(/manual\s+`?([a-z0-9-]+)#(\d+)/g)) {
        const target = bySlug.feature.get(slug);
        const count = target && isArr(target.data.manual) ? target.data.manual.length : 0;
        if (!target || Number(n) < 1 || Number(n) > count) errors.push(`${where}: manual reference '${slug}#${n}' does not resolve`);
      }
    }
    const manual = d.manual === undefined ? [] : d.manual;
    if (!isArr(manual)) errors.push(`${where}: 'manual' must be a list`);
    else manual.forEach((m, i) => {
      if (!str(m && m.steps)) errors.push(`${where}: manual[${i}] has empty steps`);
      if (!str(m && m.expected)) errors.push(`${where}: manual[${i}] has empty expected`);
      for (const a of (m && m.aliases) || []) {
        if (aliasOwner.has(a)) errors.push(`${where}: manual alias '${a}' is already used by ${aliasOwner.get(a)}`);
        else aliasOwner.set(a, where);
      }
    });
  }

  for (const r of requirements) {
    const d = r.data, where = r.file;
    if (str(d.status) && !REQUIREMENT_STATUS.includes(d.status)) errors.push(`${where}: unknown status '${d.status}'`);
    for (const f of ['source', 'milestone']) if (!str(d[f])) errors.push(`${where}: missing required field '${f}'`);
    if (!isArr(d.features)) errors.push(`${where}: 'features' must be a list`);
    for (const s of isArr(d.features) ? d.features : []) {
      const feat = bySlug.feature.get(s);
      if (!feat) errors.push(`${where}: unknown feature '${s}'`);
      else if (!(isArr(feat.data.requirements) && feat.data.requirements.includes(d.slug))) errors.push(`${where}: feature '${s}' does not list this requirement back`);
    }
  }

  const gherkinOwner = new Map();
  for (const f of features) {
    if (!/\.feature$/.test(String(f.data.gherkin))) continue;
    if (gherkinOwner.has(f.data.gherkin)) errors.push(`${f.file}: gherkin '${f.data.gherkin}' is also used by ${gherkinOwner.get(f.data.gherkin)}`);
    else gherkinOwner.set(f.data.gherkin, f.file);
  }

  // Requirement milestones exist in docs/milestones.md when that file is present.
  const msFile = path.join(root, 'docs/milestones.md');
  if (fs.existsSync(msFile)) {
    const known = new Set([...fs.readFileSync(msFile, 'utf8').matchAll(/^\|\s*(M\d+)\s*\|/gm)].map((m) => m[1]));
    for (const it of all) {
      const ms = String(it.data.milestone || '').trim();
      if (ms && known.size && !known.has(ms)) errors.push(`${it.file}: milestone '${ms}' is not in docs/milestones.md`);
    }
  }

  // Gherkin files nobody points at.
  const featDir = path.join(root, 'tests/features');
  if (fs.existsSync(featDir)) {
    const used = new Set(features.map((f) => f.data.gherkin));
    for (const n of fs.readdirSync(featDir).filter((n) => n.endsWith('.feature') && !n.startsWith('_'))) {
      if (!used.has(`tests/features/${n}`)) warnings.push(`tests/features/${n}: not referenced by any feature`);
    }
  }

  // Named Rust tests resolve to a `fn`.
  const fns = rustFnNames();
  for (const f of features) {
    for (const entry of Array.isArray(f.data.tests) ? f.data.tests : []) {
      for (const id of testIdentifiers(String(entry))) {
        const fn = id.split('::').pop();
        if (!fns.has(fn)) warnings.push(`${f.file}: test '${id}' has no matching fn under src/ or tests/`);
      }
    }
  }
  return { errors, warnings };
}

// ---- generation ------------------------------------------------------------------------------

const cell = (s) => String(s ?? '').replace(/\r?\n+/g, ' ').replace(/\|/g, '\\|').trim();
const list = (v) => (Array.isArray(v) ? v : []);
const BANNER = '<!-- GENERATED — do not edit; source: docs/features/*.md, docs/requirements/*.md (see docs/registry.md) -->';

function splitNotes(body) {
  const i = body.search(/^## Notes\s*$/m);
  if (i < 0) return { description: body, notes: '' };
  return { description: body.slice(0, i).trim(), notes: body.slice(i).replace(/^## Notes\s*/, '').trim() };
}
const withAlias = (d) => `${d.slug}${list(d.aliases).length ? ` (${list(d.aliases).join(', ')})` : ''}`;

function renderFeatures(features, requirements) {
  const milestones = path.join(root, 'docs/milestones.md');
  const out = [BANNER, '', '# Glide requirements and feature registry', '',
    'Generated by `node scripts/registry.mjs`. Edit `docs/features/<slug>.md` or `docs/requirements/<slug>.md`, never this file.',
    'Scheme and rules: [registry.md](registry.md).', ''];
  if (fs.existsSync(milestones)) out.push(fs.readFileSync(milestones, 'utf8').trim(), '');
  out.push('## Requirements', '', '| Slug (legacy ID) | Requirement | Source / basis | Features | Milestone |', '| --- | --- | --- | --- | --- |');
  for (const r of requirements) {
    const d = r.data;
    out.push(`| ${cell(withAlias(d))} | ${cell(r.body || d.title)} | ${cell(d.source)} | ${cell(list(d.features).join(', '))} | ${cell(d.milestone)} |`);
  }
  out.push('', '## Features', '', '| Slug (legacy ID) | Feature | Description | Status | Requirements | Gherkin | Tests | MCP | Notes |', '| --- | --- | --- | --- | --- | --- | --- | --- | --- |');
  for (const f of features) {
    const d = f.data, { description, notes } = splitNotes(f.body);
    out.push(`| ${cell(withAlias(d))} | ${cell(d.title)} | ${cell(description)} | ${cell(d.status)} | ${cell(list(d.requirements).join(', '))} | ${cell(d.gherkin)} | ${cell(list(d.tests).join(', '))} | ${cell(d.mcp)} | ${cell(notes)} |`);
  }
  return out.join('\n') + '\n';
}

function renderManual(features) {
  const out = [BANNER, '', '# Manual checks', '',
    'Behavior that automated tests cannot reach (a real window, OS input, focus). Run with `cargo run --release` before marking the owning feature `done`.',
    'Each entry lives in the `manual:` list of its feature file; the ID is `<feature-slug>#<n>`.', '',
    '| ID (legacy) | Feature | Steps | Expected |', '| --- | --- | --- | --- |'];
  for (const f of features) {
    list(f.data.manual).forEach((m, i) => {
      const al = list(m.aliases).length ? ` (${m.aliases.join(', ')})` : '';
      const covers = list(m.covers).length ? `, ${m.covers.join(', ')}` : '';
      out.push(`| ${cell(`${f.data.slug}#${i + 1}${al}`)} | ${cell(f.data.slug + covers)} | ${cell(m.steps)} | ${cell(m.expected)} |`);
    });
  }
  return out.join('\n') + '\n';
}

// ---- main ------------------------------------------------------------------------------------

const reqs = loadDir('requirement', 'docs/requirements');
const feats = loadDir('feature', 'docs/features');
const { errors, warnings } = validate({
  features: feats.items, requirements: reqs.items, loadErrors: [...reqs.errors, ...feats.errors],
});
for (const w of warnings) console.warn(`warning: ${w}`);
for (const e of errors) console.error(`error: ${e}`);
const manualCount = feats.items.reduce((n, f) => n + list(f.data.manual).length, 0);
console.log(`${feats.items.length} features, ${reqs.items.length} requirements, ${manualCount} manual checks, ${warnings.length} warnings, ${errors.length} errors`);
if (errors.length) process.exit(1);
if (!checkOnly) {
  fs.writeFileSync(path.join(root, 'docs/features.md'), renderFeatures(feats.items, reqs.items));
  fs.writeFileSync(path.join(root, 'docs/manual-tests.md'), renderManual(feats.items));
  console.log('wrote docs/features.md and docs/manual-tests.md');
}
