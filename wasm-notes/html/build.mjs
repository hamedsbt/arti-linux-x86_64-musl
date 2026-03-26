#!/usr/bin/env node
// Build the complete annotated diff HTML from fragments.
// Usage: node build.mjs > ../annotated-diff.html

import { readFileSync, writeFileSync, existsSync } from 'node:fs';
import { execSync } from 'node:child_process';
import annotations from './annotations.mjs';

const DIR = new URL('.', import.meta.url).pathname;

function escapeHtml(s) {
  return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
}

function langForFile(filename) {
  if (filename.endsWith('.rs')) return 'diff-rust';
  if (filename.endsWith('.toml')) return 'diff-toml';
  if (filename.endsWith('.js') || filename.endsWith('.mjs')) return 'diff-javascript';
  if (filename.endsWith('.sh')) return 'diff-bash';
  if (filename.endsWith('.html')) return 'diff-markup';
  if (filename.endsWith('.css')) return 'diff-css';
  if (filename.endsWith('.yml') || filename.endsWith('.yaml')) return 'diff-bash';
  if (filename.endsWith('.svg')) return 'diff-markup';
  return 'diff';
}

function parseDiff(diffText) {
  const files = [];
  let current = null;
  for (const line of diffText.split('\n')) {
    if (line.startsWith('diff --git')) {
      const match = line.match(/b\/(.+)$/);
      const path = match ? match[1] : 'unknown';
      current = { path, lines: [] };
      files.push(current);
    } else if (current) {
      if (line.startsWith('index ') || line.startsWith('---') || line.startsWith('+++') ||
          line.startsWith('new file') || line.startsWith('old mode') || line.startsWith('new mode') ||
          line.startsWith('deleted file') || line.startsWith('similarity') || line.startsWith('rename') ||
          line.startsWith('Binary')) {
        continue;
      }
      current.lines.push(line);
    }
  }
  return files;
}

function generateFragment(annot) {
  const diffFile = `${DIR}${annot.id}.diff`;
  if (!existsSync(diffFile)) return '';

  const diff = readFileSync(diffFile, 'utf8');
  if (!diff.trim()) return '';

  const files = parseDiff(diff);

  let html = '';
  html += `<section class="diff-class" id="${annot.id}">\n`;
  html += `  <h2>${escapeHtml(annot.title)} <span class="stats">${escapeHtml(annot.stats)}</span></h2>\n`;
  html += `  <div class="annotation">\n    <p>${annot.annotation}</p>\n  </div>\n`;

  for (const file of files) {
    const displayPath = file.path
      .replace(/^crates\/[^/]+\//, '')
      .replace(/^examples\//, '')
      .replace(/^scripts\//, '')
      .replace(/^\.github\//, '');

    const fileAnnot = annot.files?.[displayPath] || annot.files?.[file.path] || {};
    const contentLines = file.lines.filter(l => !l.startsWith('@@'));
    const adds = contentLines.filter(l => l.startsWith('+')).length;
    const dels = contentLines.filter(l => l.startsWith('-')).length;
    const totalChanged = adds + dels;
    const collapsed = fileAnnot.collapsed ?? (totalChanged > 80);
    const openAttr = collapsed ? '' : ' open';
    const lang = langForFile(file.path);
    const annotText = fileAnnot.annotation || '';
    const statsStr = adds && dels ? `+${adds} -${dels}` : adds ? `+${adds}` : `-${dels}`;

    html += `  <details${openAttr}>\n`;
    html += `    <summary>\n`;
    html += `      <span class="filename">${escapeHtml(displayPath)}</span>\n`;
    html += `      <span class="stats">${statsStr}</span>\n`;
    if (annotText) {
      html += `      <span class="file-annotation">${escapeHtml(annotText)}</span>\n`;
    }
    html += `    </summary>\n`;
    html += `    <pre><code class="language-${lang}">`;
    html += escapeHtml(file.lines.join('\n'));
    html += `</code></pre>\n`;
    html += `  </details>\n`;
  }

  html += `</section>\n`;
  return html;
}

// Build TOC
let toc = '<nav>\n  <h3>Contents</h3>\n';
for (const annot of annotations) {
  const diffFile = `${DIR}${annot.id}.diff`;
  if (!existsSync(diffFile) || !readFileSync(diffFile, 'utf8').trim()) continue;
  toc += `  <a href="#${annot.id}">${escapeHtml(annot.title)}</a>\n`;
}
toc += '</nav>\n';

// Assemble
const head = readFileSync(`${DIR}wrapper-head.html`, 'utf8');
const foot = readFileSync(`${DIR}wrapper-foot.html`, 'utf8');

let body = head + toc;
for (const annot of annotations) {
  body += generateFragment(annot);
}
body += foot;

writeFileSync(`${DIR}../annotated-diff.html`, body);
console.log(`Written to wasm-notes/annotated-diff.html (${(body.length / 1024).toFixed(0)} KB)`);
