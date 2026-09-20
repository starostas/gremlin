export const pages = {
  'README.md': {
    id: 'index',
    route: '/',
    title: 'Gremlin',
    description: 'Search for small integer programs that match observed behavior.',
    template: 'splash',
    next: false,
    hero: {
      title: 'gremlin',
      tagline: 'a compiler to discover non-trivial optimizations',
      actions: [
        {
          text: 'Explore experiments',
          link: '/experiments/',
          icon: 'right-arrow'
        },
        {
          text: 'Read the docs',
          link: '/usage/',
          variant: 'minimal'
        }
      ]
    }
  },
  'docs/experiments.mdx': {
    id: 'experiments',
    route: '/experiments/',
    title: 'Experiments',
    description: 'Browser experiments and synthesis case studies built with Gremlin.'
  },
  'docs/experiments/shader-detective.mdx': {
    id: 'experiments/shader-detective',
    route: '/experiments/shader-detective/',
    title: 'Shader Detective',
    description: 'Recover a hidden packed-color transform from observed input and output colors.'
  },
  'docs/experiments/landing-lab.mdx': {
    id: 'experiments/landing-lab',
    route: '/experiments/landing-lab/',
    title: 'Landing Lab',
    description: 'Search a bounded controller for a toy spacecraft landing simulation.'
  },
  'docs/experiments/tiny-robot.mdx': {
    id: 'experiments/tiny-robot',
    route: '/experiments/tiny-robot/',
    title: 'Tiny Robot',
    description: 'Evolve a small stateful controller and test it in editable maze rooms.'
  },
  'docs/experiments/shader-sculptor.mdx': {
    id: 'experiments/shader-sculptor',
    route: '/experiments/shader-sculptor/',
    title: 'Shader Sculptor',
    description: 'Approximate an image with a layered, executable drawing program.'
  },
  'docs/experiments/orbit-forge.mdx': {
    id: 'experiments/orbit-forge',
    route: '/experiments/orbit-forge/',
    title: 'Orbit Forge',
    description: 'Search numerical solver strategies for Kepler’s equation.'
  },
  'docs/usage.md': {
    id: 'usage',
    route: '/usage/',
    title: 'Getting started',
    description: 'Build Gremlin, run a first program, and start a small search.',
    prev: false,
    next: false
  },
  'docs/run-programs.md': {
    id: 'usage/run-a-program',
    route: '/usage/run-a-program/',
    title: 'Run a program',
    description: 'Check and execute a Gremlin program with fixed-width integer inputs.'
  },
  'docs/synthesize.md': {
    id: 'usage/synthesize',
    route: '/usage/synthesize/',
    title: 'Synthesize a program',
    description: 'Configure a search, rank candidates, and interpret its output.'
  },
  'docs/results-and-resume.md': {
    id: 'usage/results-and-resume',
    route: '/usage/results-and-resume/',
    title: 'Inspect and continue a search',
    description: 'Read search artifacts, understand evidence, and safely resume work.'
  },
  'docs/compiled-targets.md': {
    id: 'advanced/compiled-targets',
    route: '/advanced/compiled-targets/',
    title: 'Compiled targets and verification',
    description: 'Refine a binary target and verify a candidate with the supported workflows.'
  },
  'docs/semantics.md': {
    id: 'semantics',
    route: '/semantics/',
    title: 'Language reference',
    description: 'The Gremlin language, execution model, and fixed-width integer semantics.'
  },
  'docs/language/types-and-literals.md': {
    id: 'language/types-and-literals',
    route: '/language/types-and-literals/',
    title: 'Types and literals',
    description: 'Value types, typed literals, bindings, and source limits.'
  },
  'docs/language/operators.md': {
    id: 'language/operators',
    route: '/language/operators/',
    title: 'Operators',
    description: 'Fixed-width arithmetic, comparisons, traps, and evaluation order.'
  },
  'docs/language/control-flow.md': {
    id: 'language/control-flow',
    route: '/language/control-flow/',
    title: 'Control flow',
    description: 'Structured branches and loops in Gremlin source.'
  },
  'docs/language/functions.md': {
    id: 'language/functions',
    route: '/language/functions/',
    title: 'Functions and calls',
    description: 'Module entry points, typed calls, recursion, and backend support.'
  },
  'docs/language/execution.md': {
    id: 'language/execution',
    route: '/language/execution/',
    title: 'Execution model',
    description: 'Steps, outcomes, and call-depth limits.'
  },
  'docs/language/ir-and-corpus.md': {
    id: 'language/ir-and-corpus',
    route: '/language/ir-and-corpus/',
    title: 'IR and corpus format',
    description: 'IR validation, canonical source, and corpus identity.'
  },
  'docs/support.md': {
    id: 'support',
    route: '/support/',
    title: 'Supported features',
    description: 'Supported execution, CUDA, verification, and compilation capabilities.'
  },
  'docs/comparators.md': {
    id: 'comparators',
    route: '/comparators/',
    title: 'Custom scoring',
    description: 'Use built-in and custom fitness comparators without changing correctness checks.'
  },
  'docs/development.md': {
    id: 'development',
    route: '/development/',
    title: 'Development',
    description: 'Build, test, and contribute to Gremlin.'
  }
};
