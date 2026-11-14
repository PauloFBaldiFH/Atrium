// SPDX-License-Identifier: GPL-3.0-or-later
// Cords of the staff catalog listing (docs/specs/catalogo/listagem.md, sketch v9): one
// Bezier curve from each parent block to each child block, with the label the server wrote
// in Fluent. A block may have several parents: `data-parent` lists them separated by
// spaces and `data-cord-label` lists their labels, in the same order, separated by `|`.
// No inline script or style: the CSP forbids them, so everything is set from this module.
const SVG = "http://www.w3.org/2000/svg";
// Below this horizontal distance a label would cover the blocks it joins.
const MIN_LABEL_DISTANCE = 45;

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

function measure(family) {
  const box = family.getBoundingClientRect();
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
      links.push({
        x1,
        y1,
        x2: to.left - box.left,
        y2: to.top + to.height / 2 - box.top,
        kind: cordClass(parent),
        label: show ? label : "",
      });
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
  for (const { x1, y1, x2, y2, kind, label } of links) {
    const dx = Math.abs(x2 - x1) * 0.48;
    paths.push(
      element("path", { class: kind, d: `M ${x1} ${y1} C ${x1 + dx} ${y1}, ${x2 - dx} ${y2}, ${x2} ${y2}` }),
    );
    if (!label || Math.abs(x2 - x1) <= MIN_LABEL_DISTANCE) continue;
    const midX = (x1 + x2) / 2;
    const midY = (y1 + y2) / 2;
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
  for (const family of document.querySelectorAll("li.family:not([data-cords])")) {
    family.dataset.cords = "";
    observer.observe(family);
  }
}

watch();
document.addEventListener("htmx:afterSettle", watch);
