#!/usr/bin/env node
// Generate an HTML fragment from a diff file and annotations JSON.
// Usage: node gen-fragment.mjs <class-id> <annotations.json> <diff-file> > fragment.html
//
// annotations.json format:
// {
//   "id": "tor-rtcompat",
//   "title": "tor-rtcompat",
//   "stats": "+972 -37",
//   "annotation": "WASM runtime implementation...",
//   "files": {
//     "src/wasm.rs": { "annotation": "Full WASM runtime", "collapsed": true },
//     "src/wasm_compat.rs": { "annotation": "Send/Sync shims" }
//   }
// }

import { readFileSync } from 'node:fs';

const annotFile = process.argv[2];
const diffFile = process.argv[3];

const annot = JSON.parse(readFileSync(annotFile, 'utf8'));
const diff = readFileSync(diffFile, 'utf8');

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
  if (filename.endsWith('.yml') || filename.endsWith('.yaml')) return 'diff-yaml';
  return 'diff';
}

// Parse diff into files
const files = [];
let current = null;
for (const line of diff.split('\n')) {
  if (line.startsWith('diff --git')) {
    const match = line.match(/b\/(.+)$/);
    const path = match ? match[1] : 'unknown';
    current = { path, lines: [] };
    files.push(current);
  } else if (current) {
    // Skip diff headers but keep hunk headers and content
    if (line.startsWith('index ') || line.startsWith('---') || line.startsWith('+++') || line.startsWith('new file') || line.startsWith('old mode') || line.startsWith('new mode') || line.startsWith('deleted file') || line.startsWith('similarity') || line.startsWith('rename') || line.startsWith('Binary')) {
      continue;
    }
    current.lines.push(line);
  }
}

// Generate HTML
let html = '';
html += `<section class="diff-class" id="${annot.id}">\n`;
html += `  <h2>${escapeHtml(annot.title)} <span class="stats">${escapeHtml(annot.stats)}</span></h2>\n`;
html += `  <div class="annotation">\n    <p>${annot.annotation}</p>\n  </div>\n`;

for (const file of files) {
  // Strip crate prefix for display
  const displayPath = file.path.replace(/^crates\/[^/]+\//, '');
  const fileAnnot = annot.files?.[displayPath] || annot.files?.[file.path] || {};
  const lineCount = file.lines.filter(l => l.startsWith('+') || l.startsWith('-')).length;
  const collapsed = fileAnnot.collapsed ?? (lineCount > 80);
  const openAttr = collapsed ? '' : ' open';
  const lang = langForFile(file.path);
  const annotText = fileAnnot.annotation || '';

  // Count additions/deletions
  const adds = file.lines.filter(l => l.startsWith('+') && !l.startsWith('+++')).length;
  const dels = file.lines.filter(l => l.startsWith('-') && !l.startsWith('---')).length;
  const statsStr = `+${adds} -${dels}`;

  html += `  <details${openAttr}>\n`;
  html += `    <summary>\n`;
  html += `      <span class="filename">${escapeHtml(displayPath)}</span>\n`;
  html += `      <span class="stats">${statsStr}</span>\n`;
  if (annotText) {
    html += `      <span class="file-annotation">${escapeHtml(annotText)}</span>\n`;
  }
  html += `    </summary>\n`;
  html += `    <pre><code class="language-${lang}">\n`;
  html += escapeHtml(file.lines.join('\n'));
  html += `\n</code></pre>\n`;
  html += `  </details>\n`;
}

html += `</section>\n`;
process.stdout.write(html);
