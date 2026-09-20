import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';
import preact from '@astrojs/preact';
import repositoryContent from './src/remark-repository-content.mjs';

const site =
  process.env.SITE_URL ??
  (process.env.VERCEL_URL ? `https://${process.env.VERCEL_URL}` : 'http://localhost:3000');

export default defineConfig({
  site,
  markdown: {
    // Adapt repository Markdown for the site while keeping its source GitHub-friendly.
    remarkPlugins: [[repositoryContent, { base: '' }]]
  },
  integrations: [
    preact(),
    starlight({
      title: 'Gremlin',
      description:
        'Documentation for Gremlin, a deterministic search system for small integer programs.',
      customCss: ['./src/styles/custom.css'],
      markdown: {
        // Content is intentionally loaded from the repository README and /docs rather than
        // Starlight's default src/content/docs directory.
        processedDirs: ['.', '..']
      },
      sidebar: [
        { label: 'Overview', slug: 'index' },
        {
          label: 'Experiments',
          items: [
            { label: 'Overview', slug: 'experiments' },
            { label: 'Shader Detective', slug: 'experiments/shader-detective' },
            { label: 'Landing Lab', slug: 'experiments/landing-lab' },
            { label: 'Tiny Robot', slug: 'experiments/tiny-robot' },
            { label: 'Shader Sculptor', slug: 'experiments/shader-sculptor' },
            { label: 'Orbit Forge', slug: 'experiments/orbit-forge' }
          ]
        },
        {
          label: 'Get started',
          items: [
            { label: 'Getting started', slug: 'usage' },
            { label: 'Run a program', slug: 'usage/run-a-program' },
            { label: 'Synthesize', slug: 'usage/synthesize' },
            { label: 'Results and resume', slug: 'usage/results-and-resume' }
          ]
        },
        {
          label: 'Reference',
          items: [
            { label: 'Language overview', slug: 'semantics' },
            { label: 'Types and literals', slug: 'language/types-and-literals' },
            { label: 'Operators', slug: 'language/operators' },
            { label: 'Control flow', slug: 'language/control-flow' },
            { label: 'Functions and calls', slug: 'language/functions' },
            { label: 'Execution model', slug: 'language/execution' },
            { label: 'IR and corpus format', slug: 'language/ir-and-corpus' },
            { label: 'Supported features', slug: 'support' }
          ]
        },
        {
          label: 'Advanced',
          items: [
            {
              label: 'Compiled targets and verification',
              slug: 'advanced/compiled-targets'
            },
            { label: 'Custom scoring', slug: 'comparators' }
          ]
        },
        {
          label: 'Contribute',
          items: [
            { label: 'Development', slug: 'development' }
          ]
        }
      ]
    })
  ]
});
