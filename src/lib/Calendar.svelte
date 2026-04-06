<!-- Copyright (C) 2026 Wim Palland

This file is part of Grimoire.

Grimoire is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, either version 3 of the License, or
(at your option) any later version.

Grimoire is distributed in the hope that it will be useful,
but WITHOUT ANY WARRANTY; without even the implied warranty of
MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
GNU General Public License for more details.

You should have received a copy of the GNU General Public License
along with Grimoire. If not, see <https://www.gnu.org/licenses/>. -->

<script>
  import { invoke } from '@tauri-apps/api/core';

  /**
   * @type {{
   *   open: boolean,
   *   onClose: () => void,
   *   onSelectNote: (note: object) => void,
   *   onRefresh: () => void,
   *   onSelectFolder: (id: number) => void,
   *   dateFormat: string
   * }}
   */
  let { open, onClose, onSelectNote, onRefresh, onSelectFolder, dateFormat = 'DD-MM-YYYY' } = $props();

  // ── Constants ──────────────────────────────────────────────────────────────

  const DOW_LABELS = ['Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat', 'Sun'];

  const MONTH_NAMES = [
    'January', 'February', 'March', 'April', 'May', 'June',
    'July', 'August', 'September', 'October', 'November', 'December',
  ];

  const MONTH_SHORT = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'];

  // Labels for the heatmap day-of-week axis (Mon = row 0).
  // Only Mon, Wed, Fri are shown to avoid crowding at 14px cell height.
  const DOW_AXIS = ['Mo', '', 'We', '', 'Fr', '', ''];

  // ── State ──────────────────────────────────────────────────────────────────

  /** @type {'calendar' | 'heatmap'} */
  let subView = $state('calendar');

  const nowObj = new Date();
  let viewYear  = $state(nowObj.getFullYear());
  let viewMonth = $state(nowObj.getMonth()); // 0-indexed

  /** @type {Array<{date: string, created: number, modified: number}>} */
  let heatmapData = $state([]);

  /** @type {string | null} */
  let selectedHeatmapDay = $state(null);

  /** @type {object[]} */
  let dayNotes = $state([]);

  /** All notes in the vault — fetched independently of the current sidebar selection. */
  let allNotes = $state([]);

  // ── Date helpers ───────────────────────────────────────────────────────────

  /** Convert a Date object to a YYYY-MM-DD string in local time. */
  function toISODate(d) {
    const y = d.getFullYear();
    const m = String(d.getMonth() + 1).padStart(2, '0');
    const day = String(d.getDate()).padStart(2, '0');
    return `${y}-${m}-${day}`;
  }

  /** Format a YYYY-MM-DD string for display using the user's dateFormat preference. */
  function formatDateStr(iso) {
    const [y, m, d] = iso.split('-');
    if (dateFormat === 'DD-MM-YYYY') return `${d}-${m}-${y}`;
    if (dateFormat === 'MM-DD-YYYY') return `${m}-${d}-${y}`;
    return iso;
  }

  const todayISO = toISODate(new Date());

  // ── Calendar grid derived values ───────────────────────────────────────────

  /**
   * Set of ISO date strings that have a daily note.
   * Daily notes are identified by a title matching YYYY-MM-DD exactly.
   */
  let dailyNoteDates = $derived(
    new Set(
      allNotes
        .filter(n => !n.locked && /^\d{4}-\d{2}-\d{2}$/.test(n.title))
        .map(n => n.title)
    )
  );

  let monthLabel = $derived(`${MONTH_NAMES[viewMonth]} ${viewYear}`);

  /**
   * Flat array of cells for the calendar grid.
   * null = leading empty cell; number = day-of-month.
   */
  let calendarCells = $derived.by(() => {
    const firstDay = new Date(viewYear, viewMonth, 1);
    const daysInMonth = new Date(viewYear, viewMonth + 1, 0).getDate();
    // Convert JS day (0=Sun) to Mon-first offset (0=Mon ... 6=Sun)
    const startOffset = (firstDay.getDay() + 6) % 7;
    const cells = [];
    for (let i = 0; i < startOffset; i++) cells.push(null);
    for (let d = 1; d <= daysInMonth; d++) cells.push(d);
    return cells;
  });

  function cellISO(day) {
    return `${viewYear}-${String(viewMonth + 1).padStart(2, '0')}-${String(day).padStart(2, '0')}`;
  }

  function isCellToday(day) {
    return cellISO(day) === todayISO;
  }

  function isCellFuture(day) {
    const t = new Date();
    t.setHours(0, 0, 0, 0);
    return new Date(viewYear, viewMonth, day) > t;
  }

  // ── Heatmap derived values ─────────────────────────────────────────────────

  /**
   * 2-D grid of heatmap cells: weeks[col][row].
   * col 0 = oldest week (leftmost), col 52 = current week (rightmost).
   * row 0 = Monday, row 6 = Sunday.
   * A cell is null when the date is in the future, otherwise { date, created, modified, total, level }.
   */
  let heatmapWeeks = $derived.by(() => {
    const today = new Date();
    today.setHours(0, 0, 0, 0);

    // Monday of current week
    const dow = today.getDay(); // 0=Sun ... 6=Sat
    const daysToMon = dow === 0 ? 6 : dow - 1;
    const currentMonday = new Date(today);
    currentMonday.setDate(today.getDate() - daysToMon);

    // Start: Monday 52 weeks ago
    const startMonday = new Date(currentMonday);
    startMonday.setDate(startMonday.getDate() - 52 * 7);

    const activityMap = new Map(heatmapData.map(d => [d.date, d]));
    const maxTotal = heatmapData.reduce((m, d) => Math.max(m, d.created + d.modified), 0);

    const weeks = [];
    for (let w = 0; w < 53; w++) {
      const week = [];
      for (let d = 0; d < 7; d++) {
        const cellDate = new Date(startMonday);
        cellDate.setDate(startMonday.getDate() + w * 7 + d);
        if (cellDate > today) {
          week.push(null);
        } else {
          const iso = toISODate(cellDate);
          const act = activityMap.get(iso);
          const total = (act?.created ?? 0) + (act?.modified ?? 0);
          week.push({
            date: iso,
            created: act?.created ?? 0,
            modified: act?.modified ?? 0,
            total,
            level: heatLevel(total, maxTotal),
          });
        }
      }
      weeks.push(week);
    }
    return weeks;
  });

  /**
   * Bucket total activity into one of 5 heat levels (0–4) relative to the
   * maximum activity found in the dataset. This mirrors GitHub's approach
   * so the chart always has visual contrast regardless of absolute counts.
   */
  function heatLevel(total, maxTotal) {
    if (total === 0 || maxTotal === 0) return 0;
    const r = total / maxTotal;
    if (r <= 0.25) return 1;
    if (r <= 0.5)  return 2;
    if (r <= 0.75) return 3;
    return 4;
  }

  /**
   * Month labels positioned above the heatmap grid.
   * Each label records the column index where a new month starts.
   */
  let monthLabels = $derived.by(() => {
    const labels = [];
    let lastMonth = -1;
    heatmapWeeks.forEach((week, colIdx) => {
      const first = week.find(c => c !== null);
      if (!first) return;
      const m = new Date(first.date).getMonth();
      if (m !== lastMonth) {
        lastMonth = m;
        labels.push({ col: colIdx, label: MONTH_SHORT[m] });
      }
    });
    return labels;
  });

  // ── Effects ────────────────────────────────────────────────────────────────

  $effect(() => {
    if (!open) return;
    // Reset UI state each time the overlay is opened.
    const now = new Date();
    viewYear = now.getFullYear();
    viewMonth = now.getMonth();
    selectedHeatmapDay = null;
    dayNotes = [];
    invoke('get_activity_heatmap')
      .then(data => (heatmapData = data))
      .catch(() => {});
    invoke('list_notes', { all: true })
      .then(data => (allNotes = data))
      .catch(() => {});
  });

  // ── Navigation ─────────────────────────────────────────────────────────────

  function prevYear() { viewYear--; }
  function nextYear() { viewYear++; }

  function goToday() {
    const now = new Date();
    viewYear = now.getFullYear();
    viewMonth = now.getMonth();
  }

  function prevMonth() {
    if (viewMonth === 0) { viewMonth = 11; viewYear--; }
    else viewMonth--;
  }

  function nextMonth() {
    if (viewMonth === 11) { viewMonth = 0; viewYear++; }
    else viewMonth++;
  }

  // ── Actions ────────────────────────────────────────────────────────────────

  async function openDay(day) {
    if (isCellFuture(day)) return;
    try {
      const note = await invoke('get_or_create_daily_note', { dateStr: cellISO(day) });
      await onRefresh();
      // Select the Daily Notes folder in the sidebar so the note is visible.
      if (note.folder_id != null) onSelectFolder(note.folder_id);
      onSelectNote(note);
      onClose();
    } catch (e) {
      console.error('Calendar: failed to open daily note', e);
    }
  }

  async function selectHeatmapDay(iso) {
    selectedHeatmapDay = iso;
    try {
      dayNotes = await invoke('get_notes_for_day', { dateStr: iso });
    } catch (e) {
      dayNotes = [];
    }
  }
</script>

<div class="calendar-overlay" role="dialog" aria-modal="true" aria-label="Calendar">

  <!-- ── Header ───────────────────────────────────────────────────────────── -->
  <div class="cal-header">
    <div class="cal-tabs" role="tablist">
      <button
        class="cal-tab"
        class:active={subView === 'calendar'}
        role="tab"
        aria-selected={subView === 'calendar'}
        onclick={() => (subView = 'calendar')}
      >Calendar</button>
      <button
        class="cal-tab"
        class:active={subView === 'heatmap'}
        role="tab"
        aria-selected={subView === 'heatmap'}
        onclick={() => (subView = 'heatmap')}
      >Activity</button>
    </div>
    <button class="cal-close" onclick={onClose}>✕ Close</button>
  </div>

  <!-- ── Body ─────────────────────────────────────────────────────────────── -->
  <div class="cal-body">

    {#if subView === 'calendar'}
      <!-- ── Monthly calendar grid ──────────────────────────────────────── -->
      <div class="cal-view">
        <div class="cal-month-nav">
          <button class="cal-nav-btn" onclick={prevYear} aria-label="Previous year">«</button>
          <button class="cal-nav-btn" onclick={prevMonth} aria-label="Previous month">‹</button>
          <span class="cal-month-label">{monthLabel}</span>
          <button class="cal-nav-btn" onclick={nextMonth} aria-label="Next month">›</button>
          <button class="cal-nav-btn" onclick={nextYear} aria-label="Next year">»</button>
          <button class="cal-today-btn" onclick={goToday} aria-label="Go to today">Today</button>
        </div>

        <div class="cal-grid" role="grid" aria-label={monthLabel}>
          <!-- Day-of-week header row -->
          {#each DOW_LABELS as label}
            <div class="cal-dow" role="columnheader">{label}</div>
          {/each}

          <!-- Day cells -->
          {#each calendarCells as cell}
            {#if cell === null}
              <div class="cal-cell cal-empty" role="gridcell"></div>
            {:else}
              <button
                class="cal-cell cal-day"
                class:today={isCellToday(cell)}
                class:future={isCellFuture(cell)}
                role="gridcell"
                aria-label="{cellISO(cell)}{isCellToday(cell) ? ' (today)' : ''}{dailyNoteDates.has(cellISO(cell)) ? ', has note' : ''}"
                onclick={() => openDay(cell)}
              >
                <span class="cal-day-num">{cell}</span>
                {#if dailyNoteDates.has(cellISO(cell))}
                  <span class="cal-dot" aria-hidden="true"></span>
                {/if}
              </button>
            {/if}
          {/each}
        </div>

        <p class="cal-hint">Click a day to open or create a daily note.</p>
      </div>

    {:else}
      <!-- ── Activity heatmap ──────────────────────────────────────────── -->
      <div class="heat-view">

        <!-- Left: heatmap + legend -->
        <div class="heat-left">
          <div class="heat-layout">

            <!-- Day-of-week axis labels -->
            <div class="heat-dow-axis" aria-hidden="true">
              <div class="heat-dow-month-spacer"></div>
              {#each DOW_AXIS as label}
                <div class="heat-dow-label">{label}</div>
              {/each}
            </div>

            <!-- Months row + grid columns -->
            <div class="heat-right">
              <!-- Month labels aligned to column positions -->
              <div class="heat-months" aria-hidden="true">
                {#each heatmapWeeks as _week, wi}
                  <div class="heat-col-label">
                    {#each monthLabels as ml}
                      {#if ml.col === wi}{ml.label}{/if}
                    {/each}
                  </div>
                {/each}
              </div>

              <!-- 53 × 7 grid -->
              <div class="heat-grid" role="grid" aria-label="Activity heatmap">
                {#each heatmapWeeks as week, _wi}
                  <div class="heat-col" role="row">
                    {#each week as cell}
                      {#if cell === null}
                        <div class="heat-cell heat-empty" role="gridcell"></div>
                      {:else}
                        <button
                          class="heat-cell heat-{cell.level}"
                          class:heat-clickable={cell.total > 0}
                          class:heat-selected={cell.date === selectedHeatmapDay}
                          role="gridcell"
                          aria-label="{cell.date}: {cell.created} created, {cell.modified} modified"
                          onclick={() => cell.total > 0 && selectHeatmapDay(cell.date)}
                        >
                          {#if cell.total > 0}
                            <div class="heat-tooltip" aria-hidden="true">
                              {formatDateStr(cell.date)}<br>
                              {cell.created} created · {cell.modified} modified
                            </div>
                          {/if}
                        </button>
                      {/if}
                    {/each}
                  </div>
                {/each}
              </div>

              <!-- Legend -->
              <div class="heat-legend" aria-hidden="true">
                <span>Less</span>
                <div class="heat-cell heat-0"></div>
                <div class="heat-cell heat-1"></div>
                <div class="heat-cell heat-2"></div>
                <div class="heat-cell heat-3"></div>
                <div class="heat-cell heat-4"></div>
                <span>More</span>
              </div>
            </div>

          </div>
        </div>

        <!-- Right: always-visible detail sidebar -->
        <div class="heat-sidebar" role="region" aria-label="Day detail" aria-live="polite">
          {#if selectedHeatmapDay}
            <div class="heat-detail-header">
              <span class="heat-detail-date">{formatDateStr(selectedHeatmapDay)}</span>
              <button class="heat-detail-close" onclick={() => { selectedHeatmapDay = null; dayNotes = []; }}>✕</button>
            </div>
            {#if dayNotes.length === 0}
              <p class="heat-detail-empty">No notes found for this day.</p>
            {:else}
              <div class="heat-detail-list" role="list">
                {#each dayNotes as note}
                  <button
                    class="heat-note-pill"
                    onclick={() => { onSelectNote(note); onClose(); }}
                  >{note.locked ? '🔒 Locked note' : note.title}</button>
                {/each}
              </div>
            {/if}
          {:else}
            <p class="heat-sidebar-empty">Select a day on the heatmap to see its notes.</p>
          {/if}
        </div>

      </div>
    {/if}

  </div>
</div>

<style>
  /* ── Overlay ─────────────────────────────────────────────────────────────── */

  .calendar-overlay {
    position: fixed;
    top: 36px;
    left: 0;
    right: 0;
    bottom: 0;
    z-index: 100;
    background: var(--bg);
    display: flex;
    flex-direction: column;
    font-family: var(--sans);
  }

  /* ── Header ─────────────────────────────────────────────────────────────── */

  .cal-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 20px;
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
  }

  .cal-tabs {
    display: flex;
    gap: 0;
  }

  .cal-tab {
    background: none;
    border: none;
    border-bottom: 2px solid transparent;
    padding: 6px 14px;
    font: 13px var(--sans);
    color: var(--text);
    cursor: pointer;
    margin-bottom: -1px; /* sit on top of the header border */
  }

  .cal-tab:hover {
    color: var(--text-h);
  }

  .cal-tab.active {
    color: var(--accent);
    border-bottom-color: var(--accent);
    font-weight: 600;
  }

  .cal-close {
    padding: 5px 12px;
    border: 1px solid var(--border);
    border-radius: 5px;
    background: var(--bg2);
    color: var(--text);
    font-size: 12px;
    cursor: pointer;
  }

  .cal-close:hover {
    background: var(--bg3);
    color: var(--text-h);
  }

  /* ── Body ───────────────────────────────────────────────────────────────── */

  .cal-body {
    flex: 1;
    overflow: hidden;
    display: flex;
    align-items: stretch;
  }

  /* ── Calendar sub-view ──────────────────────────────────────────────────── */

  .cal-view {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 20px;
    width: 100%;
    max-width: 420px;
    margin: 40px auto;
    padding: 0 24px;
  }

  .cal-month-nav {
    display: flex;
    align-items: center;
    gap: 16px;
  }

  .cal-nav-btn {
    background: none;
    border: 1px solid var(--border);
    border-radius: 5px;
    padding: 4px 10px;
    color: var(--text-h);
    font-size: 14px;
    cursor: pointer;
  }

  .cal-today-btn {
    background: none;
    border: 1px solid var(--border);
    border-radius: 5px;
    padding: 4px 10px;
    color: var(--text);
    font: 12px var(--sans);
    cursor: pointer;
    margin-left: 6px;
  }

  .cal-today-btn:hover {
    background: var(--bg3);
    color: var(--text-h);
  }

  .cal-month-label {
    font-size: 15px;
    font-weight: 600;
    color: var(--text-h);
    min-width: 160px;
    text-align: center;
  }

  .cal-grid {
    display: grid;
    grid-template-columns: repeat(7, 1fr);
    gap: 4px;
    width: 100%;
  }

  .cal-dow {
    text-align: center;
    font-size: 11px;
    font-weight: 600;
    color: var(--text);
    text-transform: uppercase;
    letter-spacing: 0.04em;
    padding: 4px 0;
  }

  .cal-cell {
    aspect-ratio: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    border-radius: 5px;
    border: 1px solid transparent;
    background: none;
    cursor: pointer;
    gap: 2px;
    position: relative;
  }

  .cal-empty {
    cursor: default;
    pointer-events: none;
  }

  .cal-day:hover {
    background: var(--bg3);
  }

  .cal-day.today {
    border-color: var(--accent);
  }

  .cal-day.today .cal-day-num {
    color: var(--accent);
    font-weight: 700;
  }

  .cal-day.future {
    opacity: 0.35;
    cursor: default;
    pointer-events: none;
  }

  .cal-day-num {
    font-size: 13px;
    color: var(--text-h);
    line-height: 1;
  }

  .cal-dot {
    width: 4px;
    height: 4px;
    border-radius: 50%;
    background: var(--accent);
    flex-shrink: 0;
  }

  .cal-hint {
    font-size: 11px;
    color: var(--text);
    margin: 0;
    opacity: 0.7;
  }

  /* ── Heatmap sub-view ───────────────────────────────────────────────────── */

  .heat-view {
    display: flex;
    flex-direction: row;
    flex: 1;
    overflow: hidden;
  }

  /* Heatmap area — scrollable if the window is very narrow */
  .heat-left {
    flex: 1;
    overflow: auto;
    padding: 40px;
    display: flex;
    align-items: flex-start;
  }

  .heat-layout {
    display: flex;
    gap: 6px;
    align-items: flex-start;
  }

  /* Day-of-week labels on the left */
  .heat-dow-axis {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .heat-dow-month-spacer {
    height: 18px; /* matches month label row height */
    margin-bottom: 4px;
  }

  .heat-dow-label {
    width: 22px;
    height: 16px;
    font-size: 10px;
    color: var(--text);
    display: flex;
    align-items: center;
  }

  .heat-right {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  /* Month label row — one div per column, overflow visible so labels show */
  .heat-months {
    display: flex;
    gap: 3px;
    height: 18px;
  }

  .heat-col-label {
    width: 16px;
    font-size: 10px;
    color: var(--text);
    overflow: visible;
    white-space: nowrap;
  }

  /* Grid */
  .heat-grid {
    display: flex;
    gap: 3px;
  }

  .heat-col {
    display: flex;
    flex-direction: column;
    gap: 3px;
  }

  .heat-cell {
    width: 16px;
    height: 16px;
    border-radius: 3px;
    border: none;
    padding: 0;
    cursor: default;
    position: relative;
    flex-shrink: 0;
  }

  .heat-selected {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }

  .heat-empty {
    visibility: hidden;
  }

  .heat-clickable {
    cursor: pointer;
  }

  /* Heat levels — intensity increases from bg3 toward accent */
  .heat-0 { background: var(--bg3); }
  .heat-1 { background: color-mix(in srgb, var(--accent) 20%, var(--bg3)); }
  .heat-2 { background: color-mix(in srgb, var(--accent) 40%, var(--bg3)); }
  .heat-3 { background: color-mix(in srgb, var(--accent) 65%, var(--bg3)); }
  .heat-4 { background: var(--accent); }

  /* Tooltip — shown on cell hover, no JS required */
  .heat-tooltip {
    display: none;
    position: absolute;
    bottom: calc(100% + 6px);
    left: 50%;
    transform: translateX(-50%);
    background: var(--bg2);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 5px 8px;
    font-size: 11px;
    line-height: 1.5;
    color: var(--text-h);
    white-space: nowrap;
    z-index: 10;
    pointer-events: none;
  }

  .heat-cell:hover .heat-tooltip {
    display: block;
  }

  /* Legend */
  .heat-legend {
    display: flex;
    align-items: center;
    gap: 4px;
    padding-top: 8px;
    font-size: 10px;
    color: var(--text);
    justify-content: flex-end;
  }

  .heat-legend .heat-cell {
    cursor: default;
  }

  /* ── Heatmap day detail sidebar ─────────────────────────────────────────── */

  .heat-sidebar {
    width: 280px;
    flex-shrink: 0;
    border-left: 1px solid var(--border);
    overflow-y: auto;
    display: flex;
    flex-direction: column;
  }

  .heat-sidebar-empty {
    padding: 40px 20px;
    font-size: 12px;
    color: var(--text);
    margin: 0;
    text-align: center;
    opacity: 0.6;
  }

  .heat-detail-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 14px;
    border-bottom: 1px solid var(--border);
  }

  .heat-detail-date {
    font-size: 13px;
    font-weight: 600;
    color: var(--text-h);
  }

  .heat-detail-close {
    background: none;
    border: none;
    color: var(--text);
    font-size: 12px;
    cursor: pointer;
    padding: 2px 6px;
    border-radius: 3px;
  }

  .heat-detail-close:hover {
    background: var(--bg3);
    color: var(--text-h);
  }

  .heat-detail-empty {
    padding: 16px 14px;
    font-size: 12px;
    color: var(--text);
    margin: 0;
  }

  .heat-detail-list {
    padding: 10px 14px;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .heat-note-pill {
    background: none;
    border: 1px solid var(--border);
    border-radius: 5px;
    padding: 6px 10px;
    text-align: left;
    font: 13px var(--sans);
    color: var(--text-h);
    cursor: pointer;
    width: 100%;
  }

  .heat-note-pill:hover {
    background: var(--accent-bg);
    color: var(--accent);
    border-color: var(--accent);
  }
</style>
