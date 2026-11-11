/**
 * Sound effects that ship with the editor, all CC0: Kenney's Interface
 * Sounds and UI Audio packs and Juhani Junkala's 512 retro sounds, as OGG
 * under `public/sounds` with a manifest. They show in the Sounds panel
 * before any Freesound result and work without the Freesound key.
 * ATTRIBUTION.md records where each pack came from.
 */

import type { SoundEffect } from "./types";

export const BUNDLED_SOUNDS_MANIFEST = "/sounds/manifest.json";

/** Ids above this never collide with Freesound's. */
export const BUNDLED_SOUND_ID_BASE = 1_000_000_000;

export interface BundledPack {
	id: string;
	name: string;
	author: string;
	/** SPDX licence id; every bundled pack is CC0-1.0. */
	license: string;
	url: string;
}

export interface BundledSound {
	pack: string;
	name: string;
	/** Path under `/sounds/`. */
	file: string;
	duration: number;
	tags: string[];
}

export interface BundledSoundsManifest {
	packs: BundledPack[];
	sounds: BundledSound[];
}

export function isBundledSoundId({ id }: { id: number }): boolean {
	return id >= BUNDLED_SOUND_ID_BASE;
}

/** The manifest's sounds as the panel's sound effects, in manifest order. */
export function toSoundEffects({
	manifest,
}: {
	manifest: BundledSoundsManifest;
}): SoundEffect[] {
	const packs = new Map(manifest.packs.map((pack) => [pack.id, pack]));
	return manifest.sounds.map((sound, index) => {
		const pack = packs.get(sound.pack);
		return {
			id: BUNDLED_SOUND_ID_BASE + index,
			name: sound.name,
			description: pack ? `${pack.name} (${pack.license})` : sound.pack,
			url: pack?.url ?? "",
			previewUrl: `/sounds/${sound.file}`,
			downloadUrl: `/sounds/${sound.file}`,
			duration: sound.duration,
			filesize: 0,
			type: sound.file.split(".").pop() ?? "ogg",
			channels: 1,
			bitrate: 0,
			bitdepth: 16,
			samplerate: 44100,
			username: pack?.author ?? "",
			tags: [...sound.tags, sound.pack],
			license: pack?.license ?? "CC0-1.0",
			created: "",
			downloads: 0,
			rating: 0,
			ratingCount: 0,
		};
	});
}

/** Bundled sounds whose name or tags contain every word of the query. */
export function searchBundledSounds({
	sounds,
	query,
}: {
	sounds: SoundEffect[];
	query: string;
}): SoundEffect[] {
	const words = query.toLowerCase().split(/\s+/).filter(Boolean);
	if (words.length === 0) return sounds;
	return sounds.filter((sound) => {
		const haystack = `${sound.name} ${sound.tags.join(" ")} ${sound.username}`.toLowerCase();
		return words.every((word) => haystack.includes(word));
	});
}

let manifestPromise: Promise<SoundEffect[]> | null = null;

export function clearBundledSoundsCache(): void {
	manifestPromise = null;
}

/** The bundled sounds, fetched once; an empty list when the manifest is missing. */
export function fetchBundledSounds({
	fetchImpl = fetch,
}: { fetchImpl?: typeof fetch } = {}): Promise<SoundEffect[]> {
	if (!manifestPromise) {
		manifestPromise = fetchImpl(BUNDLED_SOUNDS_MANIFEST)
			.then(async (response) => {
				if (!response.ok) return [];
				const manifest = (await response.json()) as BundledSoundsManifest;
				if (!Array.isArray(manifest.packs) || !Array.isArray(manifest.sounds)) return [];
				return toSoundEffects({ manifest });
			})
			.catch(() => {
				manifestPromise = null;
				return [];
			});
	}
	return manifestPromise;
}
