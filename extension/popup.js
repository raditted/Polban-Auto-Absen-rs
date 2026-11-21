/* Polban Auto Absen — popup script
 * Klik "Mulai Absen" -> kirim pesan ke background untuk buka/aktifkan
 * tab akademik, lalu tutup popup. Flow login->nav->klik jalan di tab.
 */

const KEY_PROOFS = 'paa_proofs';
const todayStr = () => new Date().toISOString().slice(0, 10);

async function getCfg() {
  return chrome.storage.sync.get(['username', 'password', 'targetUrl']);
}

function formatHms(ts) {
  if (!ts) return '-';
  if (/^\d{2}:\d{2}:\d{2}$/.test(ts)) return ts;
  const d = new Date(ts);
  if (!isNaN(d.getTime())) return d.toTimeString().slice(0, 8);
  return String(ts).slice(0, 8);
}

function verificationLabel(verifiedBy) {
  return verifiedBy === 'dom-green' ? 'Terverifikasi Portal' : 'Tercatat di Portal';
}

function buttonStatusDescription(p) {
  if (p.buttonTextAfter) {
    return p.verifiedBy === 'dom-green'
      ? `${p.buttonTextAfter} (terkonfirmasi setelah klik)`
      : p.buttonTextAfter;
  }
  if (p.verifiedBy === 'row-cleared') return 'Tombol absen tertutup setelah klik';
  return 'Tombol hijau pada halaman absen';
}

function formatProofText(p) {
  const mhs = p.studentNama ? `${p.studentNama} (${p.nim || '-'})` : (p.nim || '-');
  return [
    'BUKTI KEHADIRAN PRESENSI POLBAN',
    `ID Bukti      : ${p.proofId}`,
    `Verifikasi    : ${verificationLabel(p.verifiedBy)}`,
    `Mata Kuliah   : ${p.nama || '-'}`,
    `Dosen         : ${p.dosen || '-'}`,
    `Jam Kuliah    : ${p.jam || '-'}`,
    `Mahasiswa     : ${mhs}`,
    `Waktu Tercatat: ${formatHms(p.recordedAt)}`,
    `Status Portal : ${buttonStatusDescription(p)}`,
    `URL Portal    : ${p.pageUrl || 'https://akademik.polban.ac.id/ajar/absen'}`,
  ].join('\n');
}

async function copyText(text) {
  if (navigator.clipboard && typeof navigator.clipboard.writeText === 'function') {
    try {
      await navigator.clipboard.writeText(text);
      return true;
    } catch (e) {}
  }
  try {
    const ta = document.createElement('textarea');
    ta.value = text;
    ta.setAttribute('readonly', '');
    ta.style.position = 'fixed';
    ta.style.opacity = '0';
    document.body.appendChild(ta);
    ta.select();
    const ok = document.execCommand('copy');
    ta.remove();
    return !!ok;
  } catch (e) {
    return false;
  }
}

const proofItemsById = new Map();

function el(tag, className, text) {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text !== undefined) node.textContent = text;
  return node;
}

function renderProofs(rawProofData) {
  const countEl = document.getElementById('proof-count');
  const emptyEl = document.getElementById('proof-empty');
  const listEl = document.getElementById('proof-list');
  if (!countEl || !emptyEl || !listEl) return;

  proofItemsById.clear();
  listEl.textContent = '';

  const today = todayStr();
  const itemsObj = (rawProofData && rawProofData._d === today && rawProofData.items && typeof rawProofData.items === 'object')
    ? rawProofData.items
    : {};
  const list = Object.values(itemsObj)
    .filter((p) => p && typeof p === 'object')
    .sort((a, b) => String(b.recordedAt || '').localeCompare(String(a.recordedAt || '')));

  if (list.length === 0) {
    countEl.hidden = true;
    listEl.hidden = true;
    emptyEl.hidden = false;
    return;
  }

  countEl.textContent = `${list.length} hadir`;
  countEl.hidden = false;
  emptyEl.hidden = true;
  listEl.hidden = false;

  const frag = document.createDocumentFragment();
  list.forEach((p, idx) => {
    const itemKey = String(p.proofId || p.sig || idx);
    proofItemsById.set(itemKey, p);

    const card = el('div', 'proof-item');

    const top = el('div', 'proof-top');
    top.appendChild(el('div', 'proof-mk', p.nama || '-'));
    top.appendChild(el('span', 'badge b-green', verificationLabel(p.verifiedBy)));
    card.appendChild(top);

    const metaParts = [];
    if (p.jam) metaParts.push(p.jam);
    metaParts.push(`Tercatat ${formatHms(p.recordedAt)}`);
    card.appendChild(el('div', 'proof-meta', metaParts.join(' · ')));

    const actions = el('div', 'proof-actions');
    const detailBtn = el('button', 'proof-btn', 'Detail');
    detailBtn.type = 'button';
    detailBtn.setAttribute('data-action', 'toggle-detail');
    detailBtn.setAttribute('data-proof-key', itemKey);

    const copyBtn = el('button', 'proof-btn copy', 'Salin');
    copyBtn.type = 'button';
    copyBtn.setAttribute('data-action', 'copy-proof');
    copyBtn.setAttribute('data-proof-key', itemKey);

    actions.appendChild(detailBtn);
    actions.appendChild(copyBtn);
    card.appendChild(actions);

    const detailBox = el('div', 'proof-detail');
    detailBox.hidden = true;
    const dl = document.createElement('dl');
    const mhs = p.studentNama ? `${p.studentNama} (${p.nim || '-'})` : (p.nim || '-');
    [
      ['ID Bukti', p.proofId || '-'],
      ['Dosen', p.dosen || '-'],
      ['NIM', mhs],
      ['Portal', buttonStatusDescription(p)],
    ].forEach(([k, v]) => {
      dl.appendChild(el('dt', '', k));
      dl.appendChild(el('dd', '', v));
    });
    detailBox.appendChild(dl);
    card.appendChild(detailBox);

    frag.appendChild(card);
  });

  listEl.appendChild(frag);
}

// Single event delegation on #proof-list
document.getElementById('proof-list').addEventListener('click', async (ev) => {
  const btn = ev.target && ev.target.closest('button[data-action]');
  if (!btn) return;
  const act = btn.getAttribute('data-action');
  const key = btn.getAttribute('data-proof-key');
  const p = key && proofItemsById.get(key);
  if (!p) return;

  if (act === 'toggle-detail') {
    const item = btn.closest('.proof-item');
    const detail = item && item.querySelector('.proof-detail');
    if (!detail) return;
    detail.hidden = !detail.hidden;
    btn.textContent = detail.hidden ? 'Detail' : 'Tutup';
  } else if (act === 'copy-proof') {
    const ok = await copyText(formatProofText(p));
    if (ok) {
      btn.textContent = 'Tersalin';
      btn.classList.add('copied');
      setTimeout(() => {
        btn.textContent = 'Salin';
        btn.classList.remove('copied');
      }, 2000);
    }
  }
});

// Tampilkan status kredensial & bukti kehadiran hari ini (single read)
(async () => {
  const [c, localData] = await Promise.all([
    getCfg(),
    chrome.storage.local.get(KEY_PROOFS).catch(() => ({})),
  ]);
  document.getElementById('nim').textContent = c.username ? c.username : '(belum diisi)';
  if (!c.username || !c.password) {
    document.getElementById('warn').style.display = 'block';
  }
  renderProofs(localData && localData[KEY_PROOFS]);
})();

document.getElementById('open-options').addEventListener('click', () => {
  chrome.runtime.openOptionsPage();
});

document.getElementById('start').addEventListener('click', async () => {
  const c = await getCfg();
  if (!c.username || !c.password) {
    document.getElementById('warn').style.display = 'block';
    return; // jangan tutup popup kalau kredensial belum ada
  }

  // Tandai flow dipicu user (content script membaca ini)
  await chrome.storage.local.set({ paa_state: 'triggered' });

  // Minta background membuka/mengaktifkan tab akademik (lebih andal daripada
  // chrome.tabs dari popup, dan bekerja walau ada popup/alert yang memblokir).
  try {
    await chrome.runtime.sendMessage({ type: 'PAA_START' });
  } catch (e) {
    // Fallback: buka langsung kalau message gagal
    try { await chrome.tabs.create({ url: 'https://akademik.polban.ac.id/', active: true }); } catch {}
  }

  window.close(); // tutup popup -> flow berjalan di tab
});
