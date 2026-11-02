import { createFileRoute } from "@tanstack/react-router";
import { Header } from "@/components/header";
import { Footer } from "@/components/footer";
import { Hero } from "@/components/landing/hero";

export const Route = createFileRoute("/")({ component: Home });

function Home() {
	return (
		<div>
			<Header />
			<Hero />
			<Footer />
		</div>
	);
}
