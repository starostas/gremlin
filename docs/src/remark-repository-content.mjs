import { dirname, relative, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';
import { pages } from './site-pages.mjs';

const repositoryRoot = fileURLToPath(new URL('../../', import.meta.url));
const sourceRoutes = new Map(
  Object.entries(pages).map(([source, page]) => [source, page.route])
);
const repositorySourceUrl = 'https://github.com/starostas/gremlin/blob/main/';

function normalizePath(path) {
  return path.split(sep).join('/');
}

function splitFragment(url) {
  const fragmentIndex = url.indexOf('#');
  return fragmentIndex === -1
    ? [url, '']
    : [url.slice(0, fragmentIndex), url.slice(fragmentIndex)];
}

function isExternal(url) {
  return /^(?:[a-z][a-z\d+.-]*:|\/\/)/i.test(url);
}

function visit(node, callback) {
  callback(node);

  if (Array.isArray(node.children)) {
    for (const child of node.children) {
      visit(child, callback);
    }
  }
}

function textContent(node) {
  if (typeof node.value === 'string') return node.value;
  if (!Array.isArray(node.children)) return '';
  return node.children.map(textContent).join('');
}

function removeHomeOnlySections(tree, file) {
  const sourcePath = normalizePath(relative(repositoryRoot, file.path));
  if (sourcePath !== 'README.md') return;

  const excludedSections = new Set(['Demos', 'Documentation']);
  let includeSection = true;

  tree.children = tree.children.filter((node) => {
    if (node.type === 'heading' && node.depth === 2) {
      includeSection = !excludedSections.has(textContent(node));
    }

    return includeSection;
  });
}

/**
 * Keeps repository Markdown GitHub-friendly while adapting it for the Starlight site.
 * It supplies one page title by removing the source H1 and turns links to loaded pages
 * into base-aware site routes. The landing page omits README sections that have dedicated
 * in-site destinations. Other repository files keep linking to their GitHub source.
 */
export default function repositoryContent({ base = '' } = {}) {
  const normalizedBase = base.replace(/\/$/, '');

  return (tree, file) => {
    const firstHeading = tree.children?.findIndex(
      (node) => node.type === 'heading' && node.depth === 1
    );

    if (firstHeading !== undefined && firstHeading >= 0) {
      tree.children.splice(firstHeading, 1);
    }

    if (!file.path) return;

    removeHomeOnlySections(tree, file);

    visit(tree, (node) => {
      if (node.type !== 'link' || typeof node.url !== 'string' || isExternal(node.url)) {
        return;
      }

      const [target, fragment] = splitFragment(node.url);
      if (!target) return;

      const absoluteTarget = resolve(dirname(file.path), target);
      const sourcePath = normalizePath(relative(repositoryRoot, absoluteTarget));

      if (sourcePath === '..' || sourcePath.startsWith('../')) return;

      const route = sourceRoutes.get(sourcePath);
      if (route) {
        node.url = `${normalizedBase}${route}${fragment}`;
      } else if (/\.(?:md|gremlin)$/i.test(sourcePath)) {
        node.url = `${repositorySourceUrl}${sourcePath}${fragment}`;
      }
    });
  };
}
