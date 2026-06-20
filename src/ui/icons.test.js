import { describe, expect, it } from "vitest";
import { icon } from "./icons.js";

describe("icon", () => {
  it("renders a labelled SVG with a real title node", () => {
    const host = document.createElement("div");
    host.innerHTML = icon("send", { label: "Send message" });

    const svg = host.querySelector("svg");
    expect(svg).toBeTruthy();
    expect(svg.getAttribute("role")).toBe("img");
    expect(svg.getAttribute("aria-label")).toBe("Send message");
    expect(svg.querySelector("title")?.textContent).toBe("Send message");
    expect(svg.querySelector("path")).toBeTruthy();
  });

  it("renders unlabelled icons as decorative", () => {
    const host = document.createElement("div");
    host.innerHTML = icon("settings");

    const svg = host.querySelector("svg");
    expect(svg).toBeTruthy();
    expect(svg.getAttribute("aria-hidden")).toBe("true");
    expect(svg.querySelector("title")).toBeNull();
  });
});
