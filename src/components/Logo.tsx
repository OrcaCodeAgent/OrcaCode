import { useLayoutEffect, useRef, useState } from "react";

function onDarkBackground(el: HTMLElement): boolean {
  let node: HTMLElement | null = el.parentElement;
  while (node) {
    const match = getComputedStyle(node).backgroundColor.match(/rgba?\((\d+),\s*(\d+),\s*(\d+)(?:,\s*([\d.]+))?\)/);
    if (match) {
      const alpha = match[4] === undefined ? 1 : Number(match[4]);
      if (alpha > 0.35) {
        const luminance = (0.2126 * Number(match[1]) + 0.7152 * Number(match[2]) + 0.0722 * Number(match[3])) / 255;
        return luminance < 0.55;
      }
    }
    node = node.parentElement;
  }
  return true;
}

export function Logo({ className, label = "Orca" }: { className?: string; label?: string }) {
  const ref = useRef<HTMLSpanElement>(null);
  const [onDark, setOnDark] = useState(true);

  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const update = () => setOnDark(onDarkBackground(el));
    update();
    const observer = new MutationObserver(update);
    observer.observe(document.documentElement, { attributes: true, attributeFilter: ["class", "style"] });
    let node: HTMLElement | null = el.parentElement;
    while (node) {
      observer.observe(node, { attributes: true, attributeFilter: ["class", "style"] });
      node = node.parentElement;
    }
    return () => observer.disconnect();
  }, []);

  return (
    <span
      ref={ref}
      role="img"
      aria-label={label}
      className={`inline-block shrink-0 ${className ?? ""}`}
      style={{
        backgroundColor: onDark ? "#ffffff" : "#111111",
        WebkitMaskImage: "url(/logo-mask.png)",
        maskImage: "url(/logo-mask.png)",
        WebkitMaskRepeat: "no-repeat",
        maskRepeat: "no-repeat",
        WebkitMaskPosition: "center",
        maskPosition: "center",
        WebkitMaskSize: "contain",
        maskSize: "contain",
      }}
    />
  );
}
