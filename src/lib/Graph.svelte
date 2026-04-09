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
  import { onMount, onDestroy } from 'svelte';
  import {
    forceSimulation,
    forceLink,
    forceManyBody,
    forceCenter,
    forceCollide,
  } from 'd3-force';

  // ── Props ─────────────────────────────────────────────────────────────────

  // Called when the user clicks a node. Passes the note id.
  let { onSelectNote, activeNoteId = null, theme = 'system' } = $props();

  // ── Spellbook: background stars ───────────────────────────────────────
  // A fixed set of tiny dots scattered across the sky. Generated once on
  // resize from a simple pseudo-random seed so they stay stable between frames.
  let bgStars = [];

  function generateStars(w, h) {
    const count = Math.floor((w * h) / 2400); // roughly 1 star per 2400px²
    const stars = [];
    let seed = 42;
    for (let i = 0; i < count; i++) {
      seed = (seed * 16807 + 7) % 2147483647; // Park-Miller LCG
      const x = (seed / 2147483647) * w;
      seed = (seed * 16807 + 7) % 2147483647;
      const y = (seed / 2147483647) * h;
      seed = (seed * 16807 + 7) % 2147483647;
      const r = 0.4 + (seed / 2147483647) * 1.1;  // radius 0.4–1.5
      seed = (seed * 16807 + 7) % 2147483647;
      const a = 0.2 + (seed / 2147483647) * 0.5;   // alpha 0.2–0.7
      stars.push({ x, y, r, a });
    }
    bgStars = stars;
  }

  // ── Canvas and simulation state ───────────────────────────────────────────

  let canvas;           // bound to the <canvas> element
  let width  = $state(800);
  let height = $state(600);
  let simulation = null;
  let rafId = null;     // requestAnimationFrame handle — kept so we can cancel

  // Working copies mutated by d3-force.
  let nodes = [];
  let links = [];

  // Hover and drag state
  let hoveredNode = null;
  let dragNode    = null;
  let dragOffsetX = 0;
  let dragOffsetY = 0;

  // ── Draw ──────────────────────────────────────────────────────────────────

  function draw() {
    if (!canvas) return;
    const ctx = canvas.getContext('2d');
    const dpr = window.devicePixelRatio || 1;
    const isSpellbook = theme === 'spellbook';

    // Read the graph-overlay's own CSS variables (not :root) so spellbook
    // overrides on .graph-overlay take effect automatically.
    const overlay = canvas.parentElement;
    const cs = getComputedStyle(overlay);
    const colBorder   = cs.getPropertyValue('--border').trim();
    const colAccent   = cs.getPropertyValue('--accent').trim();
    const colAccentBg = cs.getPropertyValue('--accent-bg').trim();
    const colText     = cs.getPropertyValue('--text-h').trim();
    const colBg       = cs.getPropertyValue('--bg2').trim();
    const colNode     = cs.getPropertyValue('--bg3').trim();

    ctx.clearRect(0, 0, canvas.width, canvas.height);

    // ── Spellbook: background stars ───────────────────────────────────
    if (isSpellbook) {
      for (const star of bgStars) {
        ctx.globalAlpha = star.a;
        ctx.fillStyle = '#f0e0c0';
        ctx.beginPath();
        ctx.arc(star.x * dpr, star.y * dpr, star.r * dpr, 0, Math.PI * 2);
        ctx.fill();
      }
      ctx.globalAlpha = 1;
    }

    // ── Edges / constellation lines ──────────────────────────────────
    ctx.save();
    if (isSpellbook) {
      ctx.strokeStyle = 'rgba(200, 151, 42, 0.3)';
      ctx.lineWidth = 1 * dpr;
      ctx.shadowColor = 'rgba(200, 151, 42, 0.15)';
      ctx.shadowBlur = 6 * dpr;
    } else {
      ctx.strokeStyle = colBorder;
      ctx.lineWidth = 1 * dpr;
      ctx.globalAlpha = 0.6;
    }
    for (const link of links) {
      const s = link.source;
      const t = link.target;
      if (!s || !t) continue;
      ctx.beginPath();
      ctx.moveTo(s.x * dpr, s.y * dpr);
      ctx.lineTo(t.x * dpr, t.y * dpr);
      ctx.stroke();
    }
    ctx.restore();

    // ── Nodes / stars ────────────────────────────────────────────────
    const NODE_R = 6 * dpr;
    for (const node of nodes) {
      const isActive  = node.id === activeNoteId;
      const isHovered = node === hoveredNode;
      const nx = node.x * dpr;
      const ny = node.y * dpr;

      if (isSpellbook) {
        // ── Star glow: radial gradient from bright core outward ──────
        const glowR = (isActive ? 18 : isHovered ? 14 : 10) * dpr;
        const grad = ctx.createRadialGradient(nx, ny, 0, nx, ny, glowR);

        if (isActive) {
          grad.addColorStop(0,   'rgba(200, 151, 42, 0.95)');
          grad.addColorStop(0.3, 'rgba(200, 151, 42, 0.5)');
          grad.addColorStop(1,   'rgba(200, 151, 42, 0)');
        } else if (isHovered) {
          grad.addColorStop(0,   'rgba(240, 224, 192, 0.85)');
          grad.addColorStop(0.3, 'rgba(200, 151, 42, 0.35)');
          grad.addColorStop(1,   'rgba(200, 151, 42, 0)');
        } else {
          grad.addColorStop(0,   'rgba(240, 224, 192, 0.7)');
          grad.addColorStop(0.4, 'rgba(200, 151, 42, 0.15)');
          grad.addColorStop(1,   'rgba(200, 151, 42, 0)');
        }

        ctx.fillStyle = grad;
        ctx.beginPath();
        ctx.arc(nx, ny, glowR, 0, Math.PI * 2);
        ctx.fill();

        // Hard bright core
        const coreR = (isActive ? 3.5 : isHovered ? 3 : 2) * dpr;
        ctx.fillStyle = isActive ? '#f0d060' : '#f0e0c0';
        ctx.beginPath();
        ctx.arc(nx, ny, coreR, 0, Math.PI * 2);
        ctx.fill();

        // 4-point sparkle rays on active or hovered stars
        if (isActive || isHovered) {
          const rayLen = (isActive ? 14 : 10) * dpr;
          const rayW   = 0.5 * dpr;
          ctx.save();
          ctx.strokeStyle = isActive ? 'rgba(240, 208, 96, 0.6)' : 'rgba(240, 224, 192, 0.4)';
          ctx.lineWidth = rayW;
          for (let angle = 0; angle < Math.PI * 2; angle += Math.PI / 2) {
            ctx.beginPath();
            ctx.moveTo(nx, ny);
            ctx.lineTo(nx + Math.cos(angle) * rayLen, ny + Math.sin(angle) * rayLen);
            ctx.stroke();
          }
          ctx.restore();
        }
      } else {
        // ── Default (non-spellbook) rendering ────────────────────────
        ctx.beginPath();
        ctx.arc(nx, ny, NODE_R, 0, Math.PI * 2);

        if (isActive) {
          ctx.fillStyle = colAccent;
        } else if (isHovered) {
          ctx.fillStyle = colAccentBg;
        } else {
          ctx.fillStyle = colNode;
        }
        ctx.fill();

        ctx.strokeStyle = isActive ? colAccent : colBorder;
        ctx.lineWidth = (isActive ? 2 : 1) * dpr;
        ctx.stroke();
      }

      // Label — only draw when hovered or active
      if (isHovered || isActive) {
        const fontFamily = isSpellbook
          ? "'Palatino Linotype', 'Book Antiqua', Palatino, Georgia, serif"
          : 'system-ui, sans-serif';
        ctx.font = `${11 * dpr}px ${fontFamily}`;
        ctx.fillStyle = colText;
        ctx.textAlign = 'center';

        if (isSpellbook) {
          // Subtle glow behind label text for readability
          ctx.save();
          ctx.shadowColor = 'rgba(200, 151, 42, 0.4)';
          ctx.shadowBlur = 4 * dpr;
          ctx.fillText(node.title, nx, (node.y - 14) * dpr);
          ctx.restore();
        } else {
          ctx.fillText(node.title, nx, (node.y - 10) * dpr);
        }
      }
    }
  }

  // ── Simulation loop ───────────────────────────────────────────────────────

  function tick() {
    draw();
    // d3-force sets simulation.alpha() to decay toward 0. When it's very low
    // the graph has settled and we stop the rAF loop to save CPU.
    if (simulation && simulation.alpha() > simulation.alphaMin()) {
      rafId = requestAnimationFrame(tick);
    } else {
      rafId = null;
      draw(); // final frame
    }
  }

  function restartTick() {
    if (!rafId) {
      rafId = requestAnimationFrame(tick);
    }
  }

  // ── Data loading ──────────────────────────────────────────────────────────

  async function loadGraph() {
    const [rawNodes, rawEdges] = await invoke('get_graph_data');

    // d3-force mutates node objects to add x, y, vx, vy.
    nodes = rawNodes.map(n => ({ ...n }));

    // d3-force wants link objects with `source`/`target` as node references or ids.
    links = rawEdges.map(e => ({ source: e.source, target: e.target }));

    if (simulation) simulation.stop();

    // Cast to any to avoid type inference issues from d3-force's .id() return type.
    // d3-force ships without bundled TypeScript declarations.
    const linkForce = /** @type {any} */ (forceLink(links).id(d => d.id));
    linkForce.distance(80);
    linkForce.strength(0.4);

    simulation = forceSimulation(nodes)
      .force('link', linkForce)
      .force('charge', forceManyBody()
        .strength(-120)
        // Barnes-Hut approximation: only compute repulsion for nodes within
        // this distance. Keeps O(n log n) instead of O(n²) for large graphs.
        .distanceMax(300))
      .force('center', forceCenter(width / 2, height / 2))
      .force('collide', forceCollide(14))
      .alphaDecay(0.02)   // slower decay = smoother settling
      .on('tick', restartTick);

    restartTick();
  }

  // ── Resize ────────────────────────────────────────────────────────────────

  function resize() {
    if (!canvas) return;
    const dpr = window.devicePixelRatio || 1;
    const rect = canvas.parentElement.getBoundingClientRect();
    width  = rect.width;
    height = rect.height;
    canvas.style.width  = width  + 'px';
    canvas.style.height = height + 'px';
    canvas.width  = width  * dpr;
    canvas.height = height * dpr;
    if (theme === 'spellbook') generateStars(width, height);
    if (simulation) {
      simulation.force('center', forceCenter(width / 2, height / 2));
      simulation.alpha(0.3).restart();
      restartTick();
    } else {
      draw();
    }
  }

  // ── Hit testing ──────────────────────────────────────────────────────────

  function nodeAt(x, y) {
    const R = 10; // slightly larger than drawn radius for easier clicking
    for (const node of nodes) {
      const dx = node.x - x;
      const dy = node.y - y;
      if (dx * dx + dy * dy <= R * R) return node;
    }
    return null;
  }

  function canvasCoords(e) {
    const rect = canvas.getBoundingClientRect();
    return { x: e.clientX - rect.left, y: e.clientY - rect.top };
  }

  // ── Pointer events ────────────────────────────────────────────────────────

  function onPointerMove(e) {
    const { x, y } = canvasCoords(e);
    if (dragNode) {
      dragNode.x  = x - dragOffsetX;
      dragNode.y  = y - dragOffsetY;
      dragNode.fx = dragNode.x;
      dragNode.fy = dragNode.y;
      simulation?.alpha(0.1).restart();
      restartTick();
    } else {
      const prev = hoveredNode;
      hoveredNode = nodeAt(x, y);
      canvas.style.cursor = hoveredNode ? 'pointer' : 'default';
      if (hoveredNode !== prev) draw();
    }
  }

  function onPointerDown(e) {
    if (e.button !== 0) return;
    const { x, y } = canvasCoords(e);
    const node = nodeAt(x, y);
    if (node) {
      dragNode    = node;
      dragOffsetX = x - node.x;
      dragOffsetY = y - node.y;
      canvas.setPointerCapture(e.pointerId);
    }
  }

  function onPointerUp(e) {
    if (dragNode) {
      // Release the node so the simulation can move it again.
      dragNode.fx = null;
      dragNode.fy = null;
      dragNode = null;
      canvas.releasePointerCapture(e.pointerId);
    }
  }

  function onClick(e) {
    const { x, y } = canvasCoords(e);
    const node = nodeAt(x, y);
    if (node) onSelectNote?.(node.id);
  }

  // ── Lifecycle ─────────────────────────────────────────────────────────────

  const resizeObserver = new ResizeObserver(resize);

  onMount(async () => {
    resize();
    resizeObserver.observe(canvas.parentElement);
    await loadGraph();
  });

  onDestroy(() => {
    if (rafId) cancelAnimationFrame(rafId);
    if (simulation) simulation.stop();
    resizeObserver.disconnect();
  });
</script>

<canvas
  bind:this={canvas}
  aria-label="Note relationship graph. Use the mouse or touch to pan, zoom, and select nodes."
  onpointermove={onPointerMove}
  onpointerdown={onPointerDown}
  onpointerup={onPointerUp}
  onclick={onClick}
></canvas>
