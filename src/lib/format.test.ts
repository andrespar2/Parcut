import { describe, expect, it } from "vitest";
import {
  defaultPartTitle,
  formatBytes,
  formatDuration,
  isValidHttpUrl,
  normalizeHashtags,
  parseHashtagInput,
  partsToSeconds,
  resolutionLabel,
  timeParts,
} from "./format";

describe("formatDuration", () => {
  it("muestra minutos y segundos bajo una hora", () => {
    expect(formatDuration(0)).toBe("00:00");
    expect(formatDuration(95)).toBe("01:35");
    expect(formatDuration(59.6)).toBe("01:00");
  });

  it("incluye horas cuando son necesarias", () => {
    expect(formatDuration(3661)).toBe("01:01:01");
  });

  it("tolera valores inválidos", () => {
    expect(formatDuration(Number.NaN)).toBe("00:00");
    expect(formatDuration(-4)).toBe("00:00");
  });
});

describe("formatBytes", () => {
  it("convierte a unidades legibles", () => {
    expect(formatBytes(0)).toBe("0 B");
    expect(formatBytes(1024)).toBe("1.0 KB");
    expect(formatBytes(5 * 1024 * 1024)).toBe("5.0 MB");
  });
});

describe("hashtags", () => {
  it("limpia, separa y elimina duplicados", () => {
    expect(normalizeHashtags(["#Viaje, #colombia #viaje #Colombia 2026!"])).toEqual([
      "Viaje",
      "colombia",
      "2026",
    ]);
  });

  it("procesa entradas individuales", () => {
    expect(parseHashtagInput("  #uno dos  ")).toEqual(["uno", "dos"]);
    expect(parseHashtagInput("###")).toEqual([]);
  });
});

describe("URL", () => {
  it("solo acepta http y https", () => {
    expect(isValidHttpUrl("https://youtube.com/watch?v=1")).toBe(true);
    expect(isValidHttpUrl("http://example.com/video")).toBe(true);
    expect(isValidHttpUrl("file:///C:/video.mp4")).toBe(false);
    expect(isValidHttpUrl("javascript:alert(1)")).toBe(false);
    expect(isValidHttpUrl("no es una url")).toBe(false);
  });
});

describe("títulos y tiempo", () => {
  it("genera el título por defecto con el número de parte real", () => {
    expect(defaultPartTitle("Mi viaje por Colombia", 7)).toBe(
      "Mi viaje por Colombia - PARTE 7",
    );
    expect(defaultPartTitle("", 2)).toBe("Video - PARTE 2");
  });

  it("convierte entre partes y segundos", () => {
    expect(partsToSeconds(0, 1, 30)).toBe(90);
    expect(partsToSeconds(1, 0, 5)).toBe(3605);
    expect(partsToSeconds(Number.NaN, 0, 10)).toBe(10);
    expect(timeParts(3725)).toEqual({ hours: 1, minutes: 2, seconds: 5 });
  });

  it("etiqueta resoluciones conocidas", () => {
    expect(resolutionLabel(1920, 1080)).toContain("Full HD");
    expect(resolutionLabel(0, 0)).toBe("—");
  });
});
