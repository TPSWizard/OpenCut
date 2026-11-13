"use client";

import { ArrowDown, ArrowRight } from "lucide-react";
import Link from "@/compat/link";
import { useSiteT } from "@/site/use-site-t";
import { Button } from "../ui/button";
import { Handlebars } from "./handlebars";
import { ProductShot } from "./product-shot";

export function Hero() {
	const t = useSiteT();

	return (
		<section className="relative isolate overflow-hidden px-4 pt-14 pb-16 text-center sm:pt-20 md:pb-24">
			<div aria-hidden="true" className="landing-grid absolute inset-0 -z-10" />

			<div className="mx-auto flex w-full max-w-3xl flex-col items-center">
				<p className="text-muted-foreground flex items-center gap-2.5 font-mono text-xs tracking-wide uppercase">
					<span
						aria-hidden="true"
						className="inline-block h-3 w-1 rounded-full bg-[var(--trim)]"
					/>
					{t("Free and open source. No account needed.")}
				</p>

				<div className="mt-7 inline-block text-4xl font-bold tracking-tighter md:text-[4rem]">
					<h1>
						{t("The open source")}{" "}
						<span className="sr-only">{t("Video editor")}</span>
					</h1>
					<Handlebars
						startLabel={t("Trim the start of the headline")}
						endLabel={t("Trim the end of the headline")}
					>
						{t("Video editor")}
					</Handlebars>
				</div>

				<p className="text-muted-foreground mx-auto mt-10 max-w-xl text-base font-light tracking-wide sm:text-xl">
					{t(
						"A simple but powerful video editor that gets the job done. It runs in your browser, and your footage stays on your device.",
					)}
				</p>

				<div className="mt-8 flex flex-col items-center gap-4 sm:flex-row sm:gap-6">
					<Button asChild size="lg" className="h-11 text-base">
						<Link href="/projects">
							{t("Check it out")}
							<ArrowRight className="ml-0.5" />
						</Link>
					</Button>
					<Button
						asChild
						variant="text"
						className="text-muted-foreground hover:text-foreground h-11 px-1 text-base hover:opacity-100"
					>
						<a href="#features">
							{t("See what it can do")}
							<ArrowDown />
						</a>
					</Button>
				</div>
			</div>

			<ProductShot />
		</section>
	);
}
