// SPDX-License-Identifier: GPL-3.0-or-later
// Cords of the staff catalog listing (docs/specs/catalogo/listagem.md): one Bezier curve
// from each block to its parent block, with the label the server wrote in Fluent. No
// inline script or style: the CSP forbids them, so everything is set from this module.
const SVG = "http://www.w3.org/2000/svg";

function draw(family) {
  let svg = family.querySelector(":scope > svg.cords");
  if (!svg) {
    svg = document.createElementNS(SVG, "svg");
    svg.setAttribute("class", "cords");
    svg.setAttribute("aria-hidden", "true");
    family.prepend(svg);
  }
  // Every position is read before anything is written, so layout runs once.
  const box = family.getBoundingClientRect();
  const links = [];
  const ports = new Map();
  for (const child of family.querySelectorAll("[data-parent]")) {
    const id = child.dataset.parent;
    const parent = family.querySelector(`[data-node="${id}"]`);
    if (!parent) continue;
    const from = parent.getBoundingClientRect();
    const to = child.getBoundingClientRect();
    if (to.left < from.right) continue; // stacked on a narrow screen: no cord
    const port = {
      x: from.right - box.left,
      y: from.top + Math.min(from.height / 2, 48) - box.top,
    };
    // One label per parent: its children share the meaning of the link.
    const labelled = ports.has(id);
    ports.set(id, port);
    links.push({
      x1: port.x,
      y1: port.y,
      x2: to.left - box.left,
      y2: to.top + Math.min(to.height / 2, 24) - box.top,
      label: labelled ? "" : child.dataset.cordLabel || "",
    });
  }
  svg.setAttribute("width", String(box.width));
  svg.setAttribute("height", String(box.height));
  const shapes = [];
  for (const link of links) {
    const d = Math.max(12, (link.x2 - link.x1) / 2);
    const path = document.createElementNS(SVG, "path");
    path.setAttribute(
      "d",
      `M ${link.x1} ${link.y1} C ${link.x1 + d} ${link.y1}, ${link.x2 - d} ${link.y2}, ${link.x2} ${link.y2}`,
    );
    shapes.push(path);
    if (link.label) {
      const text = document.createElementNS(SVG, "text");
      text.setAttribute("x", String(link.x1 + 6));
      text.setAttribute("y", String(link.y1 - 7));
      text.textContent = link.label;
      shapes.push(text);
    }
  }
  for (const port of ports.values()) {
    const dot = document.createElementNS(SVG, "circle");
    dot.setAttribute("cx", String(port.x));
    dot.setAttribute("cy", String(port.y));
    dot.setAttribute("r", "5");
    shapes.push(dot);
  }
  svg.replaceChildren(...shapes);
}

// Redrawn whenever a row changes size: new font, window, wrapped text, or a row that
// becomes visible (rows off screen are not laid out).
const observer = new ResizeObserver((entries) => {
  for (const entry of entries) draw(entry.target);
});

function watch() {
  for (const family of document.querySelectorAll("li.family:not([data-cords])")) {
    family.dataset.cords = "";
    observer.observe(family);
  }
}

watch();
document.addEventListener("htmx:afterSettle", watch);
