import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';

export default defineConfig({
  site: 'https://starostas.github.io',
  base: '/gremlin',
  integrations: [
    starlight({
      title: 'gremlin',
      description:
        'Documentation for gremlin, a deterministic search system for small integer programs.',
      customCss: ['./src/styles/custom.css'],
      sidebar: [
        { label: 'Overview', slug: 'index' },
        {
          label: 'Start here',
          items: [
            { label: 'Getting started', slug: 'getting-started' },
            { label: 'How it works', slug: 'how-it-works' },
            { label: 'Project status', slug: 'project-status' }
          ]
        }
      ]
    })
  ]
});
