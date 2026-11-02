import { Loader2 } from "lucide-react";

export function PageLoading({ label = "Loading…" }: { label?: string }) {
	return (
		<div className="bg-background flex h-screen w-screen items-center justify-center">
			<div className="flex flex-col items-center gap-4">
				<Loader2 className="text-muted-foreground size-8 animate-spin" />
				<p className="text-muted-foreground text-sm">{label}</p>
			</div>
		</div>
	);
}
