import * as React from "react";
import { cva, type VariantProps } from "class-variance-authority";
import { cn } from "../../lib/utils";
const variants = cva(
  "inline-flex h-9 items-center justify-center gap-2 rounded-md px-3 text-sm font-medium transition-all disabled:pointer-events-none disabled:opacity-45 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[#ff5b45]/30",
  {
    variants: {
      variant: {
        default: "bg-[#171716] text-white hover:bg-[#32312f]",
        accent: "bg-[#ff5b45] text-white hover:bg-[#e84c39]",
        ghost: "hover:bg-black/[.055]",
        outline: "border border-black/12 bg-white hover:border-black/25",
      },
      size: {
        default: "h-9 px-3",
        sm: "h-8 px-2.5 text-xs",
        icon: "size-9 px-0",
      },
    },
    defaultVariants: { variant: "default", size: "default" },
  },
);
export function Button({
  className,
  variant,
  size,
  ...props
}: React.ButtonHTMLAttributes<HTMLButtonElement> &
  VariantProps<typeof variants>) {
  return (
    <button className={cn(variants({ variant, size }), className)} {...props} />
  );
}
