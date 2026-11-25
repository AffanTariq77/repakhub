import { Installation, formatBytes } from "./api";

export default function Dashboard({ games }: { games: Installation[] }) {
  const unknownSize = games.filter((g) => g.sizeBytes === null).length;
  const stats = [
    { label: "Installed games", value: games.length },
    { label: "Updates pending", value: games.filter((g) => g.updateAvailable).length },
    { label: "Version unknown", value: games.filter((g) => !g.installedVersion).length },
    {
      // Sizes are only known for some sources; say so rather than present a partial sum as the total.
      label: unknownSize ? `Disk used (${unknownSize} of unknown size not counted)` : "Disk used",
      value: formatBytes(games.reduce((n, g) => n + (g.sizeBytes ?? 0), 0)),
    },
  ];

  return (
    <div className="grid grid-cols-2 xl:grid-cols-4 gap-4">
      {stats.map((s) => (
        <div key={s.label} className="rounded-xl border border-line bg-surface p-5">
          <div className="text-sm text-muted">{s.label}</div>
          <div className="mt-2 text-3xl font-semibold">{s.value}</div>
        </div>
      ))}
    </div>
  );
}
