# Welcome to the DreamWeave Mod Template!

The Mod Template is designed to be an easy-to-use, batteries-included means for you to self-host your mods and applications.
It's built on Zola, relying on its powerful templating and macros to provide a wide variety of community interaction and distribution features you won't find elsewhere.

## What it offers:

- Optional Giscus integration for comments
- Optional RSS/Atom feed generation for readers to subscribe
- Optional Auto-generated install instructions (install_instructions shortcode)
- Optional pre-formatted credits sections (credits shortcode)
- Optional Search with elasticlunr.js
- Optional page view counts provided by GoatCounter
- Automatic packaging and uploading for all mods
- Mod manager download buttons for [ModdingLinked MO2](https://www.nexusmods.com/site/mods/874) or [umo](https://modding-openmw.gitlab.io/umo/) using the modl:// spec
- Various color palettes & styles
- Unlimited taxonomy types to sort your site however you want.

## Okay, How Do I Use It?

First, decide if you want a single-page site or not. The template's optimized out of the box for single-page sites, but you can host everything on a single site if you want.

Then, fork this repository on GitHub. Clone it onto your computer, and open `config.toml`. This document contains all the global configuration values for your site, defining where it's hosted, whether to use feeds and search, etc.
It's crucial you open this file and set the correct values for your site, or it will break - badly.

### The Mandatory Stuff
First, set the base_url. The default one is `https://DreamWeave-MP.github.io/DreamWeave-Mod-Template`. It should look like this: `https://OWNER_USERNAME.github.io/REPO_NAME`, where `OWNER_USERNAME` is either the account or the organization that owns the repo.

Now, scroll down to `[extra]` and set github_username and github_project appropriately. Don't leave yet!

`giscus` is used to set up comments for your page. This is optional if you use it, but you ***really*** need to change or delete it, because it will point to the mod template's comment section out of the box.

`goatcounter_username` is mandatory to enable goatcounter. PLEASE also set this up for yourself or disable it entirely, since your page visits will be tracked by DreamWeave's goatcounter instance if you don't. Of course, you're more than welcome to let us track your page views if you want.

Finally, if you want a multi-page site, set `segment_versions = true`. This will ensure your git releases have properly set versions for each different thing you publish.

Then, open `content/_index.md`. It'll look like this:
```markdown
+++

# For multi-page sites, simply delete or comment out this config option
redirect_to = "home"
sort_by = "title"

+++
```

Delete this line: `redirect_to = "home"`. Now, your homepage will be browseable. You now have a multi-page site!

Go back to the top.

### The Fun Stuff

Decide what you want your `title`, and `logo_text` to be. These will be shown on all pages. The `generate_feeds` option determines whether to generate RSS and atom feeds, and enabling `build_search_index` will make your site searchable.

If you want to use additional *taxonomies*, for sorting your pages and posts, you may do so under the `taxonomies` section. By default, the only built-in taxonomy is `tags`, effectively for sorting mods by category.

By default, the top bar is disabled, but you can re-enable it by uncommenting `menu_items` and adding your own entries to it.

## The Rest

Now your site's up and running. You've got the basics down! There's a bit more for you to learn, though.
Check out `content/home/index.md` and `content/simplified/index.md`. Here you'll see examples of what your pages can look like (and also more documentation on how to use the mod template).
You *need* to build frontmatter for every mod. That's the section between `---` or `+++`. `simplified/index.md` contains the most minimal frontmatter possible, whereas `home/index.md` contains all possible fields.
Your frontmatter may be yaml, using `---`, or TOML, using `+++` before and after your frontmatter.

For more info, and guides on expanding the site yourself, check out [Zola's Docs](https://www.getzola.org/documentation/getting-started/overview/).

Thanks for checking out the DreamWeave Mod Template. Please consider sponsoring DreamWeave on [Ko-Fi](https://ko-fi.com/magicaldave)

## API documentation pages

The Mod Template has an opt-in documentation layout for API references, framework guides, and other code-heavy pages. Put docs in a Zola section and select the docs templates in its frontmatter:

```yaml
template: docs/section.html
page_template: docs/page.html

extra:
  api_docs: true
  docs_root: true
  docs_project_name: My Project
  docs_short_title: My Project Docs
  docs_project_path: '@/home/index.md'
  docs_repository_url: https://github.com/OWNER/REPOSITORY/tree/main/content/home
  docs_sidebar_label: Documentation
```

Declare the consumer metadata once on the documentation root. Child pages and sections inherit the selected docs templates through `page_template`; they do not need to repeat the project name, root URL, or search scope. Zola accepts YAML frontmatter as shown above as well as TOML.

API docs get a recursive collapsible project sidebar, a current-page table of contents, breadcrumbs, responsive three-column layout, and copy buttons for fenced code blocks. Guide pages are task-first: lead with the goal and a working example, then explain variations, pitfalls, and related reference. API reference pages are lookup-first: lead with the symbol or item name, signature, and one-sentence summary, then document behavior, parameters, returns, caveats, examples, and related items.

Use `{{ api_signature(value="module.function(argument)") }}` for a highlighted API signature. Use `{% usage_note(title="Compatibility") %}This behavior is optional.{% end %}` for a bordered note.
