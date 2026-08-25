export function Progress({ value }: { value: number }) {
  return (
    <div className="h-1 overflow-hidden rounded-full bg-black/8">
      <div
        className="h-full rounded-full bg-[#ff5b45] transition-[width] duration-300"
        style={{ width: `${Math.max(0, Math.min(100, value))}%` }}
      />
    </div>
  );
}
