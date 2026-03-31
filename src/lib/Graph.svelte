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
  let { onSelectNote, activeNoteId = null } = $props();

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

  // ── CSS variable helpers ──────────────────────────────────────────────────
  // Read the CSS variables once so the canvas colours match the theme.

  function cssVar(name) {
    return getComputedStyle(document.documentElement).getPropertyValue(name).trim();
  }

  // ── Draw ──────────────────────────────────────────────────────────────────

  function draw() {
    if (!canvas) return;
    const ctx = canvas.getContext('2d');
    const dpr = window.devicePixelRatio || 1;

    // Theme colours (read each frame so dark/light switches work without reload)
    const colBorder  = cssVar('--border');
    const colAccent  = cssVar('--accent');
    const colAccentBg = cssVar('--accent-bg');
    const colText    = cssVar('--text-h');
    const colBg      = cssVar('--bg2');
    const colNode    = cssVar('--bg3');

    ctx.clearRect(0, 0, canvas.width, canvas.height);

    // Edges
    ctx.save();
    ctx.strokeStyle = colBorder;
    ctx.lineWidth = 1 * dpr;
    ctx.globalAlpha = 0.6;
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

    // Nodes
    const NODE_R = 6 * dpr;
    for (const node of nodes) {
      const isActive  = node.id === activeNoteId;
      const isHovered = node === hoveredNode;

      ctx.beginPath();
      ctx.arc(node.x * dpr, node.y * dpr, NODE_R, 0, Math.PI * 2);

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

      // Label — only draw when hovered or active to avoid unreadable clutter
      if (isHovered || isActive) {
        ctx.font = `${11 * dpr}px system-ui, sans-serif`;
        ctx.fillStyle = colText;
        ctx.textAlign = 'center';
        ctx.fillText(node.title, node.x * dpr, (node.y - 10) * dpr);
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
  onpointermove={onPointerMove}
  onpointerdown={onPointerDown}
  onpointerup={onPointerUp}
  onclick={onClick}
></canvas>
