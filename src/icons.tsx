import type { SVGProps } from "react";

/**
 * One small line-icon set, drawn on a 24px grid with the same stroke so the
 * icons read as a family. They inherit the text colour.
 */
const PATHS = {
  chevronLeft: <path d="M14.5 6 8.5 12l6 6" />,
  chevronRight: <path d="m9.5 6 6 6-6 6" />,
  refresh: (
    <>
      <path d="M20 12a8 8 0 1 1-2.34-5.66L20 8.5" />
      <path d="M20 4v4.5h-4.5" />
    </>
  ),
  close: <path d="M17 7 7 17M7 7l10 10" />,
  menu: <path d="M4 7h16M4 12h16M4 17h16" />,
  plus: <path d="M12 5v14M5 12h14" />,
  leave: (
    <>
      <circle cx="12" cy="12" r="3.75" />
      <path d="M12 3.5v2M12 18.5v2M3.5 12h2M18.5 12h2M6 6l1.4 1.4M16.6 16.6 18 18M6 18l1.4-1.4M16.6 7.4 18 6" />
    </>
  ),
  edit: (
    <>
      <path d="M4.5 19.5h4l10-10a2.83 2.83 0 0 0-4-4l-10 10z" />
      <path d="m13 7 4 4" />
    </>
  ),
  download: (
    <>
      <path d="M12 4v11M7.5 10.5 12 15l4.5-4.5" />
      <path d="M5 19.5h14" />
    </>
  ),
  // Pages
  calendar: (
    <>
      <rect x="3.5" y="5" width="17" height="15.5" rx="2.5" />
      <path d="M3.5 10h17M8 3v4M16 3v4" />
      <path d="M7.5 14h2M11 14h2M14.5 14h2M7.5 17h2M11 17h2" />
    </>
  ),
  report: (
    <>
      <path d="M14 3.5H7.5a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2h9a2 2 0 0 0 2-2V8z" />
      <path d="M14 3.5V8h4.5M9 12.5h6M9 16h4" />
    </>
  ),
  review: (
    <>
      <path d="M4 6.5h10M4 12h10M4 17.5h5" />
      <path d="m13.5 17.5 2.5 2.5 4.5-5" />
    </>
  ),
  history: (
    <>
      <path d="M4 12a8 8 0 1 0 2.34-5.66L4 8.5" />
      <path d="M4 4v4.5h4.5M12 8v4.5l3 1.75" />
    </>
  ),
  settings: (
    <>
      <path d="M4 7h8.5M16.5 7H20M4 17h3.5M11.5 17H20" />
      <circle cx="14.5" cy="7" r="2" />
      <circle cx="9.5" cy="17" r="2" />
    </>
  ),
} as const;

export type IconName = keyof typeof PATHS;

export function Icon({ name, size = 18, ...rest }: { name: IconName; size?: number } & SVGProps<SVGSVGElement>) {
  return (
    <svg
      className="icon"
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.75}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden
      focusable="false"
      {...rest}
    >
      {PATHS[name]}
    </svg>
  );
}

/**
 * The app icon (src-tauri/icons/source.svg) without its shadow margin, in
 * the light theme's accent blue and top-bar navy:
 * workdays filled to the daily quota line, today still filling.
 */
export function BrandMark({ size = 24 }: { size?: number }) {
  return (
    <svg className="brand-mark" width={size} height={size} viewBox="100 100 824 824" aria-hidden focusable="false">
      <defs>
        <linearGradient id="brand-tile" x1="0" y1="0" x2="0" y2="1">
          <stop offset="0" stopColor="#116699" />
          <stop offset="1" stopColor="#3e5b76" />
        </linearGradient>
      </defs>
      <rect x="100" y="100" width="824" height="824" rx="184" fill="url(#brand-tile)" />
      {/* A light rim keeps the tile apart from the navy rail it sits on. */}
      <rect x="116" y="116" width="792" height="792" rx="170" fill="none" stroke="#fff" strokeOpacity="0.5" strokeWidth="32" />
      <rect x="288" y="356" width="76" height="372" rx="20" fill="#fff" />
      <rect x="412" y="356" width="76" height="372" rx="20" fill="#fff" />
      <rect x="536" y="356" width="76" height="372" rx="20" fill="#fff" />
      <rect x="660" y="356" width="76" height="372" rx="20" fill="#fff" fillOpacity="0.18" />
      <rect x="660" y="552" width="76" height="176" rx="20" fill="#fff" />
      <rect x="248" y="310" width="528" height="36" rx="18" fill="#fff" />
      <rect x="256" y="318" width="512" height="20" rx="10" fill="#3a8a3a" />
    </svg>
  );
}
