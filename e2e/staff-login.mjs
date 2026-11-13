// SPDX-License-Identifier: GPL-3.0-or-later
// Browser smoke test (task 0.8): a real Chromium signs in to the staff interface and out.
// Expects a running server at ELZEVIR_URL with a staff account ELZEVIR_LOGIN / ELZEVIR_PASSWORD.
import { chromium } from "playwright";
import assert from "node:assert/strict";

const base = process.env.ELZEVIR_URL ?? "http://127.0.0.1:8080";
const login = process.env.ELZEVIR_LOGIN ?? "admin";
const password = process.env.ELZEVIR_PASSWORD;
assert.ok(password, "ELZEVIR_PASSWORD is required");

const browser = await chromium.launch();
try {
  const page = await browser.newPage();
  const errors = [];
  page.on("console", (m) => m.type() === "error" && errors.push(m.text()));

  await page.goto(`${base}/`);
  assert.equal(await page.locator("h1").textContent(), "Library catalog");

  await page.goto(`${base}/equipe`);
  assert.match(page.url(), /\/equipe\/login$/);

  await page.fill("#login", login);
  await page.fill("#password", "wrong-password");
  await page.click("button[type=submit]");
  assert.equal(await page.locator("[role=alert]").textContent(), "Invalid username or password.");

  await page.fill("#password", password);
  await page.click("button[type=submit]");
  await page.waitForURL(`${base}/equipe`);
  assert.equal(await page.locator("#signed-in-as").textContent(), login);

  await page.click("text=Sign out");
  await page.waitForURL(`${base}/equipe/login`);

  // The deliberate wrong password answers 401; anything else is a real problem.
  const unexpected = errors.filter((e) => !e.includes("status of 401"));
  assert.deepEqual(unexpected, [], "no console errors (CSP, missing assets)");
  console.log("e2e: staff sign-in and sign-out passed");
} finally {
  await browser.close();
}
