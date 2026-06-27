/** Paper viewer — markdown → HTML, tab routing */
const pages = {
  draft: document.getElementById('page-draft'),
  guide: document.getElementById('page-guide'),
  downloads: document.getElementById('page-downloads'),
};

function inline(s) {
  return s
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/`([^`]+)`/g, '<code>$1</code>')
    .replace(/\*\*(.+?)\*\*/g, '<strong>$1</strong>')
    .replace(/\*(.+?)\*/g, '<em>$1</em>')
    .replace(/\[(.+?)\]\((.+?)\)/g, '<a href="$2">$1</a>');
}

function renderTable(lines) {
  const cells = (row) => row.replace(/^\||\|$/g, '').split('|').map((c) => c.trim());
  const head = cells(lines[0]).map((c) => `<th>${inline(c)}</th>`).join('');
  const body = lines.slice(2).map((row) =>
    `<tr>${cells(row).map((c) => `<td>${inline(c)}</td>`).join('')}</tr>`
  ).join('');
  return `<table><thead><tr>${head}</tr></thead><tbody>${body}</tbody></table>`;
}

function mdToHtml(md) {
  const lines = md.replace(/\r\n/g, '\n').split('\n');
  const out = [];
  let i = 0;
  let para = [];

  const flushPara = () => {
    if (para.length) {
      const html = inline(para.join(' ')).replace(/\u0001\s*/g, '<br>');
      out.push(`<p>${html}</p>`);
      para = [];
    }
  };

  while (i < lines.length) {
    const line = lines[i];

    if (!line.trim()) { flushPara(); i++; continue; }

    // fenced code block
    if (/^```/.test(line.trim())) {
      flushPara();
      i++;
      const code = [];
      while (i < lines.length && !/^```/.test(lines[i].trim())) { code.push(lines[i]); i++; }
      i++; // skip closing fence
      const esc = code.join('\n').replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
      out.push(`<pre><code>${esc}</code></pre>`);
      continue;
    }

    let m;
    if ((m = line.match(/^#\s+(.+)$/))) { flushPara(); out.push(`<h1 class="paper-title">${inline(m[1])}</h1>`); i++; continue; }
    if ((m = line.match(/^##\s+(.+)$/))) { flushPara(); out.push(`<h2>${inline(m[1])}</h2>`); i++; continue; }
    if ((m = line.match(/^###\s+(.+)$/))) { flushPara(); out.push(`<h3>${inline(m[1])}</h3>`); i++; continue; }

    // table: header row + separator row
    if (/^\|.*\|$/.test(line.trim()) && i + 1 < lines.length && /^\|[-:\s|]+\|$/.test(lines[i + 1].trim())) {
      flushPara();
      const tbl = [];
      while (i < lines.length && /^\|.*\|$/.test(lines[i].trim())) { tbl.push(lines[i].trim()); i++; }
      out.push(renderTable(tbl));
      continue;
    }

    // list
    if (/^\s*-\s+/.test(line)) {
      flushPara();
      const items = [];
      while (i < lines.length && /^\s*-\s+/.test(lines[i])) {
        let item = lines[i].replace(/^\s*-\s+/, '');
        let cls = '';
        if (/^\[ \]\s+/.test(item)) { item = '☐ ' + item.replace(/^\[ \]\s+/, ''); cls = ' class="todo"'; }
        else if (/^\[x\]\s+/i.test(item)) { item = '☑ ' + item.replace(/^\[x\]\s+/i, ''); cls = ' class="todo"'; }
        items.push(`<li${cls}>${inline(item)}</li>`);
        i++;
      }
      out.push(`<ul>${items.join('')}</ul>`);
      continue;
    }

    // paragraph line (trailing "  " = markdown hard break)
    const hardBreak = /\s{2,}$/.test(line);
    para.push(line.trim() + (hardBreak ? '\u0001' : ''));
    i++;
  }
  flushPara();
  return out.join('\n');
}

async function loadMd(path, rootId) {
  const res = await fetch(path);
  const md = await res.text();
  document.getElementById(rootId).innerHTML = mdToHtml(md);
}

function show(name) {
  Object.entries(pages).forEach(([k, el]) => el.classList.toggle('active', k === name));
  document.querySelectorAll('#nav a').forEach((a) => {
    a.classList.toggle('active', a.dataset.page === name);
  });
  location.hash = name;
}

document.getElementById('nav').addEventListener('click', (e) => {
  const a = e.target.closest('a[data-page]');
  if (!a) return;
  e.preventDefault();
  show(a.dataset.page);
});

const initial = (location.hash || '#draft').slice(1);
show(['draft', 'guide', 'downloads'].includes(initial) ? initial : 'draft');

Promise.all([
  loadMd('files/draft.md', 'draft-root'),
  loadMd('files/guide.md', 'guide-root'),
]).catch((err) => {
  console.error(err);
  document.getElementById('draft-root').innerHTML =
    '<p class="note">Failed to load draft. Check paper-server is running.</p>';
});

fetch('data/results.json')
  .then((r) => r.json())
  .then((d) => {
    const loc = d?.mode_index?.locomo_full;
    if (!loc) return;
    document.getElementById('metrics-sidebar').innerHTML = `
      <li>LoCoMo evidence recall <span class="metric">${(loc.mean_evidence_recall * 100).toFixed(1)}%</span></li>
      <li>BEIR SciFact nDCG@10 <span class="metric">${d.mode_index.beir_scifact.ndcg_at_10}</span></li>
      <li>FAMB macro (index) <span class="metric">${(d.mode_index.famb.macro * 100).toFixed(0)}%</span></li>
      <li>LongMemEval-S <span class="metric">deferred</span></li>`;
  })
  .catch(() => {});
