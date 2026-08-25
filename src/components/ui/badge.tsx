import { cn } from "../../lib/utils";
export function Badge({
  children,
  className,
}: {
  children: React.ReactNode;
  className?: string;
}) {
  return (
    <span
      className={cn(
        "inline-flex rounded-full bg-black/[.055] px-2 py-0.5 text-[11px] font-medium text-black/60",
        className,
      )}
    >
      {children}
    </span>
  );
}
