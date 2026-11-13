import { createFileRoute } from "@tanstack/react-router";
import { Header } from "@/components/header";
import { Footer } from "@/components/footer";
import { Features } from "@/components/landing/features";
import { Hero } from "@/components/landing/hero";
import { Scrubber } from "@/components/landing/scrubber";
import {
	ClosingCta,
	Platforms,
	Privacy,
	Programmable,
} from "@/components/landing/sections";
import "@/components/landing/landing.css";

export const Route = createFileRoute("/")({ component: Home });

function Home() {
	return (
		<div className="landing landing-scrubber-space">
			<Header />
			<main>
				<Hero />
				<Features />
				<Privacy />
				<Platforms />
				<Programmable />
				<ClosingCta />
			</main>
			<Footer />
			<Scrubber />
		</div>
	);
}
