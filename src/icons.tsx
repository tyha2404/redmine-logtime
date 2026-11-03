import {
  CalendarDays,
  ChevronDown,
  ChevronLeft,
  ChevronRight,
  Check,
  Download,
  FileText,
  History,
  ListChecks,
  Menu,
  Pencil,
  Plus,
  RefreshCw,
  SlidersHorizontal,
  TreePalm,
  X,
  type LucideIcon,
  type LucideProps,
} from "lucide-react";

/** The app's icons, all from Lucide (lucide.dev), by the role they play here. */
const ICONS = {
  chevronDown: ChevronDown,
  chevronLeft: ChevronLeft,
  chevronRight: ChevronRight,
  check: Check,
  refresh: RefreshCw,
  close: X,
  menu: Menu,
  plus: Plus,
  leave: TreePalm,
  edit: Pencil,
  download: Download,
  // Pages
  calendar: CalendarDays,
  report: FileText,
  review: ListChecks,
  history: History,
  settings: SlidersHorizontal,
} satisfies Record<string, LucideIcon>;

export type IconName = keyof typeof ICONS;

export function Icon({ name, size = 18, ...rest }: { name: IconName; size?: number } & LucideProps) {
  const Svg = ICONS[name];
  return <Svg className="icon" size={size} strokeWidth={1.75} aria-hidden focusable="false" {...rest} />;
}

/** Apple's icon tile: 824px at 100,100, radius 185.4 with continuous corners. */
const TILE =
  "M627.36 100c103.83 0 155.75 0 195.41 20.21a185.4 185.4 0 0 1 81.02 81.02c20.21 39.66 20.21 91.58 20.21 195.41L924 627.36c0 103.83 0 155.75 -20.21 195.41a185.4 185.4 0 0 1 -81.02 81.02c-39.66 20.21 -91.58 20.21 -195.41 20.21L396.64 924c-103.83 0 -155.75 0 -195.41 -20.21a185.4 185.4 0 0 1 -81.02 -81.02c-20.21 -39.66 -20.21 -91.58 -20.21 -195.41L100 396.64c0 -103.83 0 -155.75 20.21 -195.41a185.4 185.4 0 0 1 81.02 -81.02c39.66 -20.21 91.58 -20.21 195.41 -20.21Z";

/** The Redmine mark's seven pieces, base to top (redmine_logo.svg units). */
const MARK: [string, string][] = [
  ["#0b4f78", "0,69.55 16.8,69.55 18,55.149 2.399,51.55"],
  ["#0e5c8b", "3,49.149 18,52.75 21.601,40.149 9,33.55"],
  ["#116699", "10.2,31.149 22.2,37.75 31.8,31.149 22.8,20.949"],
  ["#0b4f78", "77.399,69.55 60.601,69.55 59.399,55.149 75,51.55"],
  ["#0e5c8b", "74.399,49.149 59.399,52.75 55.8,40.149 68.399,33.55"],
  ["#116699", "67.2,31.149 55.2,37.75 45.601,31.149 54.601,20.949"],
  ["#2a7fb4", "25.8,19.149 34.8,29.949 43.2,29.949 51,19.149 43.2,16.75 34.589,16.75"],
];

/**
 * The app logo: the Redmine logo mark (© 2009 Martin Herr, CC BY-SA 2.5)
 * in the app's accent blue on a white tile, as src-tauri/icons/source.svg.
 */
export function BrandMark({ size = 24 }: { size?: number }) {
  return (
    <svg className="brand-mark" width={size} height={size} viewBox="100 100 824 824" aria-hidden focusable="false">
      <path d={TILE} fill="#fff" />
      <g transform="translate(241 220) scale(7)">
        {MARK.map(([fill, points]) => (
          <polygon key={points} fill={fill} points={points} />
        ))}
      </g>
    </svg>
  );
}
