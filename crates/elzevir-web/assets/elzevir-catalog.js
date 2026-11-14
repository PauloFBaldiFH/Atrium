// SPDX-License-Identifier: GPL-3.0-or-later
// Cords and focus of the staff catalog listing (docs/specs/catalogo/listagem.md, sketch v9): one
// Bezier curve from each parent block to each child block, with the label the server wrote
// in Fluent. A block may have several parents: `data-parent` lists them separated by
// spaces and `data-cord-label` lists their labels, in the same order, separated by `|`.
// A cord never runs behind a block: between neighbouring columns it stays in the gap, and
// when it skips a column (an authority straight to its Work, past the Hub) it crosses that
// column through a free lane above, between or below the blocks there.
// The bar's toggles hide the Hub and Authorities columns; the choice is a per-viewer
// convenience kept in localStorage. No inline script or style: the CSP forbids them, so
// everything is set from this module.
const SVG = "http://www.w3.org/2000/svg";
// Below this horizontal distance a label would cover the blocks it joins.
const MIN_LABEL_DISTANCE = 45;
// Clearance between a cord crossing a column and the blocks it passes.
const LANE_MARGIN = 10;

function element(name, attributes) {
  const node = document.createElementNS(SVG, name);
  for (const [key, value] of Object.entries(attributes)) node.setAttribute(key, String(value));
  return node;
}

function cordClass(parent) {
  if (parent.classList.contains("card-auth")) return "cord-path dashed";
  if (parent.classList.contains("card-hub") || parent.classList.contains("card-work")) {
    return "cord-path primary";
  }
  return "cord-path";
}

// The y where a cord can cross the blocks in its way: the one closest to where it is going.
function lane(target, obstacles, height) {
  const blocked = (y) => obstacles.some((o) => y > o.top - LANE_MARGIN && y < o.bottom + LANE_MARGIN);
  if (!blocked(target)) return target;
  const candidates = obstacles
    .flatMap((o) => [o.top - LANE_MARGIN, o.bottom + LANE_MARGIN])
    .filter((y) => y >= 0 && y <= height && !blocked(y));
  return candidates.reduce((best, y) => (Math.abs(y - target) < Math.abs(best - target) ? y : best), height);
}

function route(link, cards, box) {
  const { x1, x2 } = link;
  const obstacles = cards
    .filter((card) => card.left - box.left < x2 - 1 && card.right - box.left > x1 + 1)
    .map((card) => ({ left: card.left - box.left, right: card.right - box.left, top: card.top - box.top, bottom: card.bottom - box.top }));
  if (obstacles.length === 0) return link;
  const xa = Math.min(...obstacles.map((o) => o.left));
  const xb = Math.max(...obstacles.map((o) => o.right));
  if (xa - x1 < 8 || x2 - xb < 8) return link;
  return { ...link, lane: { xa, xb, y: lane(link.y2, obstacles, box.height) } };
}

function curve(x1, y1, x2, y2) {
  const dx = Math.abs(x2 - x1) * 0.48;
  return `C ${x1 + dx} ${y1}, ${x2 - dx} ${y2}, ${x2} ${y2}`;
}

function measure(family) {
  const box = family.getBoundingClientRect();
  const blocks = [...family.querySelectorAll(".card, .item-pill")];
  const rects = new Map(blocks.map((block) => [block, block.getBoundingClientRect()]));
  const links = [];
  const labelled = new Set();
  const ports = new Map();
  for (const child of family.querySelectorAll("[data-parent]")) {
    const parents = child.dataset.parent.split(" ").filter(Boolean);
    const labels = (child.dataset.cordLabel || "").split("|");
    const to = child.getBoundingClientRect();
    parents.forEach((id, index) => {
      const parent = family.querySelector(`[data-node="${id}"]`);
      if (!parent) return;
      const from = parent.getBoundingClientRect();
      if (from.width < 10 || to.width < 10 || to.left < from.right) return; // stacked
      // One label per parent and kind of link: its children share the meaning.
      const label = labels[index] || "";
      const key = `${id}|${label}`;
      const show = label !== "" && !labelled.has(key);
      if (show) labelled.add(key);
      const x1 = from.right - box.left;
      const y1 = from.top + from.height / 2 - box.top;
      ports.set(id, { x: x1, y: y1, auth: parent.classList.contains("card-auth") });
      const others = blocks.filter((block) => block !== parent && block !== child).map((block) => rects.get(block)).filter((rect) => rect.width > 0);
      const link = {
        x1,
        y1,
        x2: to.left - box.left,
        y2: to.top + to.height / 2 - box.top,
        kind: cordClass(parent),
        label: show ? label : "",
      };
      links.push(route(link, others, box));
    });
  }
  return { width: box.width, height: box.height, links, ports: [...ports.values()] };
}

function draw(family, { width, height, links, ports }) {
  let svg = family.querySelector(":scope > svg.cords");
  if (!svg) {
    svg = element("svg", { class: "cords", "aria-hidden": "true" });
    family.prepend(svg);
  }
  svg.setAttribute("width", String(width));
  svg.setAttribute("height", String(height));
  const paths = [];
  const labels = [];
  for (const { x1, y1, x2, y2, kind, label, lane: crossing } of links) {
    const d = crossing
      ? `M ${x1} ${y1} ${curve(x1, y1, crossing.xa, crossing.y)} L ${crossing.xb} ${crossing.y} ${curve(crossing.xb, crossing.y, x2, y2)}`
      : `M ${x1} ${y1} ${curve(x1, y1, x2, y2)}`;
    paths.push(element("path", { class: kind, d }));
    if (!label || Math.abs(x2 - x1) <= MIN_LABEL_DISTANCE) continue;
    // A routed cord carries its label on the straight stretch across the skipped column.
    const midX = crossing ? (crossing.xa + crossing.xb) / 2 : (x1 + x2) / 2;
    const midY = crossing ? crossing.y : (y1 + y2) / 2;
    const labelWidth = label.length * 6.2 + 12;
    labels.push(
      element("rect", { class: "cord-label-bg", x: midX - labelWidth / 2, y: midY - 9, width: labelWidth, height: 18, rx: 5 }),
    );
    const text = element("text", { class: "cord-label-text", x: midX, y: midY + 3.5, "text-anchor": "middle" });
    text.textContent = label;
    labels.push(text);
  }
  // One port on the edge of each parent, as in the sketch.
  const dots = ports.map(({ x, y, auth }) =>
    element("circle", { class: auth ? "cord-port auth" : "cord-port", cx: x, cy: y, r: 5 }),
  );
  // Labels and ports go last so no cord crosses over them.
  svg.replaceChildren(...paths, ...labels, ...dots);
}

// Redrawn whenever rows change size: new font, window, wrapped text, or a row that
// becomes visible (rows off screen are not laid out). Every position is read before
// anything is written, so layout runs once per batch.
const observer = new ResizeObserver((entries) => {
  const measured = entries.map((entry) => [entry.target, measure(entry.target)]);
  for (const [family, geometry] of measured) draw(family, geometry);
});

function watch() {
  for (const grid of document.querySelectorAll(".family-grid:not([data-cords])")) {
    grid.dataset.cords = "";
    observer.observe(grid);
  }
}

function redrawAll() {
  const families = [...document.querySelectorAll(".family-focus[open] > .family-grid")];
  const measured = families.map((family) => [family, measure(family)]);
  for (const [family, geometry] of measured) draw(family, geometry);
}

// Column toggles: `data-toggle` names the class set on the catalog when the column is hidden.
function storedHidden(name) {
  try {
    return localStorage.getItem(`elzevir.catalog.${name}`) === "hidden";
  } catch {
    return false; // storage blocked: every column starts shown
  }
}

function setHidden(catalog, button, hidden) {
  const name = button.dataset.toggle;
  catalog.classList.toggle(`hide-${name}`, hidden);
  button.setAttribute("aria-pressed", String(!hidden));
  try {
    localStorage.setItem(`elzevir.catalog.${name}`, hidden ? "hidden" : "shown");
  } catch {
    // Storage blocked: the choice lasts for this page only.
  }
}

function toggles() {
  const catalog = document.querySelector(".catalog");
  if (!catalog) return;
  for (const button of catalog.querySelectorAll("button[data-toggle]")) {
    setHidden(catalog, button, storedHidden(button.dataset.toggle));
    button.hidden = false;
    button.addEventListener("click", () => {
      setHidden(catalog, button, button.getAttribute("aria-pressed") === "true");
      redrawAll();
    });
  }
}

// Focus (listagem.md, items 28 to 32): one family open at a time, kept in the URL fragment,
// with the trail naming it. The <details> works without this module; this only adds the rules.
function trail(details) {
  const nav = document.querySelector(".trail");
  if (!nav) return;
  const family = details?.open ? details.closest("li.family") : null;
  nav.hidden = !family;
  const name = nav.querySelector(".trail-family");
  if (name) name.textContent = family?.querySelector(".row-title strong")?.textContent ?? "";
  // The level in focus, named as its column header names it.
  const level = nav.querySelector(".trail-level");
  const grid = family?.querySelector(".family-grid");
  const header = grid?.dataset.focus ? grid.querySelector(`.track-${grid.dataset.focus === "instance" ? "editions" : grid.dataset.focus} > .col-header`) : null;
  if (level) {
    level.hidden = !header;
    level.textContent = header ? `› ${header.textContent}` : "";
  }
}

// Focus on one level of the open family (listagem.md, items 37 to 41).
function clearLevel(grid) {
  delete grid.dataset.focus;
  for (const node of grid.querySelectorAll(".focused, .dimmed, [inert]")) {
    node.classList.remove("focused", "dimmed");
    node.inert = false;
  }
}

function focusLevel(card) {
  const grid = card.closest(".family-grid");
  if (!grid) return;
  const again = card.classList.contains("focused");
  clearLevel(grid);
  if (!again) {
    grid.dataset.focus = card.dataset.level;
    card.classList.add("focused");
    if (card.dataset.level === "instance") {
      // The other Instances and their items step back, so nobody touches the wrong item.
      const own = card.dataset.node;
      for (const other of grid.querySelectorAll('.card[data-level="instance"]')) {
        if (other === card) continue;
        other.classList.add("dimmed");
        other.inert = true;
      }
      for (const stack of grid.querySelectorAll(".items-stack")) {
        if (stack.querySelector(`[data-parent="${own}"]`)) continue;
        stack.classList.add("dimmed");
        stack.inert = true;
      }
    }
  }
  follow(grid);
  trail(grid.closest(".family-focus"));
}

// Cords follow the columns while they animate.
function follow(grid) {
  const until = performance.now() + 400;
  const step = () => {
    draw(grid, measure(grid));
    if (performance.now() < until) requestAnimationFrame(step);
  };
  requestAnimationFrame(step);
}

document.addEventListener("click", (event) => {
  const target = event.target;
  if (!(target instanceof Element) || target.closest("a, button, summary, details.export-menu, input, select, textarea")) return;
  const card = target.closest(".family-grid .card[data-level]");
  if (card) focusLevel(card);
});

// ← and → move to the first card of the column before or after.
function neighbour(card, step) {
  const grid = card.closest(".family-grid");
  const columns = ["auth", "hub", "work", "instance"].filter((level) =>
    grid?.querySelector(`.card[data-level="${level}"]:not([inert])`)?.getBoundingClientRect().width,
  );
  const at = columns.indexOf(card.dataset.level);
  const level = columns[at + step];
  return level ? grid.querySelector(`.card[data-level="${level}"]:not([inert])`) : null;
}

function focused(details) {
  for (const other of document.querySelectorAll(".family-focus[open]")) {
    if (other !== details) other.open = false;
  }
  const head = details.closest("li.family")?.dataset.head;
  if (head) history.replaceState(null, "", `#f-${head}`);
  trail(details);
}

function unfocused(details) {
  if (document.querySelector(".family-focus[open]")) return;
  history.replaceState(null, "", location.pathname + location.search);
  trail(details);
}

document.addEventListener(
  "toggle",
  (event) => {
    const details = event.target;
    if (!(details instanceof HTMLDetailsElement) || !details.classList.contains("family-focus")) return;
    if (details.open) focused(details);
    else unfocused(details);
  },
  true, // toggle does not bubble
);

function typing(target) {
  return target instanceof HTMLElement && (target.isContentEditable || /^(INPUT|TEXTAREA|SELECT)$/.test(target.tagName));
}

document.addEventListener("keydown", (event) => {
  if (event.defaultPrevented || event.altKey || event.ctrlKey || event.metaKey || typing(event.target)) return;
  if (event.key === "/") {
    event.preventDefault();
    document.getElementById("q")?.focus();
    return;
  }
  const card = event.target instanceof Element ? event.target.closest(".card[data-level]") : null;
  if (card && event.key === "Enter") {
    event.preventDefault();
    focusLevel(card);
    return;
  }
  if (card && (event.key === "ArrowLeft" || event.key === "ArrowRight")) {
    event.preventDefault();
    neighbour(card, event.key === "ArrowLeft" ? -1 : 1)?.focus();
    return;
  }
  if (event.key === "Escape") {
    const open = document.querySelector(".family-focus[open]");
    if (!open) return;
    const grid = open.querySelector(".family-grid");
    if (grid) clearLevel(grid);
    open.open = false;
    open.querySelector(":scope > summary")?.focus();
    return;
  }
  if (event.key !== "ArrowDown" && event.key !== "ArrowUp") return;
  const rows = [...document.querySelectorAll("summary.family-row")];
  if (rows.length === 0) return;
  const current = document.activeElement?.closest?.("li.family")?.querySelector("summary.family-row");
  const at = rows.indexOf(current);
  const next = event.key === "ArrowDown" ? Math.min(at + 1, rows.length - 1) : Math.max(at - 1, 0);
  event.preventDefault();
  rows[at === -1 ? 0 : next].focus();
});

// A link with a family's fragment opens it, when it is on the page.
function openFromFragment() {
  const id = decodeURIComponent(location.hash.slice(1));
  if (!id.startsWith("f-")) return;
  const details = document.getElementById(id)?.querySelector(".family-focus");
  if (!details) return;
  details.open = true;
  details.scrollIntoView({ block: "start" });
}

toggles();
watch();
openFromFragment();
window.addEventListener("hashchange", openFromFragment);
trail(document.querySelector(".family-focus[open]"));
document.addEventListener("htmx:afterSettle", () => {
  watch();
  trail(document.querySelector(".family-focus[open]"));
});
