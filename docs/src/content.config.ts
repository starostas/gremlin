import { defineCollection } from 'astro:content';
import { glob } from 'astro/loaders';
import { docsSchema } from '@astrojs/starlight/schema';
import { pages } from './site-pages.mjs';

type SourcePath = keyof typeof pages;

const repositoryRoot = new URL('../../', import.meta.url);

export const collections = {
  docs: defineCollection({
    loader: glob({
      base: repositoryRoot,
      pattern: Object.keys(pages),
      generateId: ({ entry, data }) => {
        const page = pages[entry as SourcePath];

        if (!page) {
          throw new Error(`No Starlight metadata is configured for ${entry}.`);
        }

        // Keep repository Markdown free of site-only frontmatter while still providing
        // the title and description Starlight requires.
        data.title = page.title;
        data.description = page.description;

        if ('template' in page && page.template) {
          data.template = page.template;
        }

        if ('hero' in page && page.hero) {
          data.hero = page.hero;
        }

        if ('next' in page) {
          data.next = page.next;
        }

        if ('prev' in page) {
          data.prev = page.prev;
        }

        return page.id;
      }
    }),
    schema: docsSchema()
  })
};
