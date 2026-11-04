import { createFileRoute } from "@tanstack/react-router";
import { Header } from "@/components/header";
import { Footer } from "@/components/footer";
import {
	ReleaseArticle,
	ReleaseChanges,
	ReleaseDescription,
	ReleaseMeta,
	ReleaseTitle,
} from "@/changelog/components/release";
import { getSortedReleases } from "@/changelog/utils";

export const Route = createFileRoute("/changelog")({ component: ChangelogPage });

function ChangelogPage() {
	const releases = getSortedReleases();

	return (
		<section className="bg-background min-h-screen">
			<Header />
			<main className="relative container mx-auto flex max-w-3xl flex-col gap-12 px-6 pt-12 pb-24 md:pt-24">
				<div className="flex flex-col gap-4">
					<h1 className="text-4xl font-bold tracking-tight">Changelog</h1>
					<p className="text-muted-foreground text-lg">
						What changed in each ConzaCut release.
					</p>
				</div>
				<div className="relative flex flex-col gap-16 sm:border-l sm:border-border sm:pl-0 [&>article]:sm:-ml-[6px]">
					{releases.map((release, index) => (
						<ReleaseArticle
							key={release.version}
							variant="list"
							isLatest={index === 0}
						>
							<ReleaseMeta release={release} />
							<ReleaseTitle as="h2">{release.title}</ReleaseTitle>
							<ReleaseDescription>{release.description}</ReleaseDescription>
							<ReleaseChanges release={release} />
						</ReleaseArticle>
					))}
					{releases.length === 0 && (
						<p className="text-muted-foreground">No releases published yet.</p>
					)}
				</div>
			</main>
			<Footer />
		</section>
	);
}
