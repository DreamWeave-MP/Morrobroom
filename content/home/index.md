---
title: My Example Mod
description: This is the description being shown on the page
date: 2025-12-27

# All taxonomies are optional,
# but help make your site more navigable if used.
# You can use any number of taxonomies you want, 
# For sorting by author, date, engine, etc etc.
taxonomies:
  tags:
    - Some
    - Tag
    - Name

extra:
  # Override the `title` for dev builds on a per-page basis. Optional.
  #dev_title: "This is what shows up when you hover over the Dev download button"

  # Use this to hide the download/version bar entirely on some pages
  hide_download_bar: false

  # If used, informs the install instructions widget how to install this mod.
  # If left unfilled, only one data directory with a slugified version of the page title is used for install instructions.
  # Content Files and fallback entries are optional.
  # Missing config= and settings.cfg configurations.
  install_info:
    data_directories:
      - 00 Main Mod
      - 01 Some Option
      - "# The definitions for what folders, plugins, etc, your mod needs"
      - "# Are in your page frontmatter."
      - 01 Some Other Option
    content_files:
      - Some.esp
      - SomeOther.esp
      - "# Be careful, though, because comment strings *must* be quoted."
      - An.omwaddon
    fallback_entries:
      "#Explanation": Try it in fallback_entries, too!
      SomeColor: 420-42-69
      SomeOtherColor: 320,220,369
    config:
      - "# Config entries are used to deploy an entire set of data directories,"
      - "# fallback-entries, and even settings.cfg changes for a whole mod."
      - "# Try it sometime!"
      - .

  # Use this if you're publishing an application on this page and not just a mod
  # This will give you more appropriate download buttons for your platform.
  # Defauts to false, since we're only hosting mods in most cases.
  is_binary: false

  # This is used to generate mod manager download links
  # as well as other metadata about this specific mod.
  # Defaults to `morrowind`, probably never actually needs filled out.
  game: morrowind

  # If your mod is also hosted on NexusMods, put the id here to link
  # back to the nexus page. Used for metadata lookups (images, description) —
  # not for direct downloads. Optional.
  nexus_id: 57511

  # NexusMods file group ID for automated uploads via the v3 API.
  # Obtain it from the "API Info" option in the Files tab of your mod page.
  # Optional — omit if you don't want automated Nexus uploads.
  #nexus_group_id: "abc123"

  # Use this field if your mod is hosted on a different repository.
  # `provider` determines the host type; defaults to `github` if omitted.
  # `giscus` is optional and overrides the site-wide comment config for this page.
  #
  # GitHub (provider defaults to 'github' when omitted):
  #offsite_host:
  #  owner: DreamWeave-MP
  #  repo: S3LightFixes
  #  giscus:
  #    repo_id: R_kgDONYe6SQ
  #    category_id: DIC_kwDONYe6Sc4C0Jkq
  #
  # GitLab (instance is optional, defaults to https://gitlab.com):
  #offsite_host:
  #  provider: gitlab
  #  owner: momw-mods
  #  repo: some-mod
  #  instance: https://gitlab.com
  #
  # Modding-OpenMW (MOMW):
  #offsite_host:
  #  provider: momw
  #  slug: go-home
  #
  # Arbitrary URL:
  #offsite_host:
  #  provider: url
  #  url: https://example.com/mods/my-mod/

  # If true, only show the frontmatter description on index pages
  # If you want to hide the summary text completely
  # set this to true, and leave the description unset.
  # Otherwise, make sure to add a 
  # <!-- more -->
  # Marker to your pages to indicate the end of the summary.
  show_only_description: false

  # Override the `title` for stable builds on a per-page basis
  stable_title: "This is what shows up when you hover over the Stable download button"

  # Not everything needs a TOC. If not, uncomment this
  #use_toc: false

  # Curently-available version of your mod/app. Required!
  version: 0.51
---

Get started using the mod template by [forking the repository](https://github.com/DreamWeave-MP/DreamWeave-Mod-Template/fork) on GitHub.
It's built with a set of sane defaults for single-page mod sites like the one you're reading now.

Want to host a website for something hosted on another repo? What about an app instead of a mod? No problem.
This page's frontmatter includes defaults for every possible value in your page configuration.

Here, you define everything from install instructions to page layout and download URLs.

<!-- more -->

{{ install_instructions(describe=true) }}

[Read the PIL](https://www.lua.org/pil/)

```lua
>>> luap
I.S3maphore.getCurrentPlaylist()
```

# Why Use the DreamWeave Mod Template?

The DreamWeave mod template gives you everything you need to make a maintainable, beautiful mod or application site at lightning speed.
Features provided include:
- Automatic packaging and uploading of stable and development builds to GitHub releases
- Various color palettes for your viewing pleasure
- Automatically-generated table of contents from H1-H3 (optional)
- Automatically-generated install instructions for ease of use (optional)
- Page view counts with goatcounter (optional)
- Comments via Giscus (optional)
- RSS/Atom feeds so your players always know what's happening (optional)
- Search with elasticlunr.min.js with (almost) zero input from you (optional)
- Supports mod manager downlods for [ModdingLinked MO2](https://www.nexusmods.com/site/mods/874) or [umo](https://modding-openmw.gitlab.io/umo/) via the modl:// spec (optional)
- Unlimited taxonomy types to sort pages by mod type, year, author, competition, whatever you can make up.

The template is built on Zola, with a heavily modified version of the terminimal theme which we've vendored out ourselves for your ease of use.

Do you think this template might be cool, but the page you're reading now is really overbuilt and too dense? 
Check out [this page](../simplified) for an example with much shorter frontmatter and *less* layout instead of more.

# So How Do I Use This Thing?

As mentioned previously, start by forking the repository on GitHub.
The mod template is optimized for single-page sites but can easily be tweaked for bigger multi-page author sites, like S3ctor's S3cret St4sh.

Firstly, start by checking out `/config.toml`. It has many important values and explanations for each.
Only the most important ill be described here.

## Setting up config.toml

### The Stuff That Will Break If You Don't Do It

First, set the `base_url`. The default one is `https://DreamWeave-MP.github.io/DreamWeave-Mod-Template`.
It should look like this: `https://OWNER_USERNAME.github.io/REPO_NAME`, where `OWNER_USERNAME` is either the account or the organization that owns the repo.

Now, scroll down to `[extra]` and set `github_username` and `github_project` appropriately. Don't leave yet!

`giscus` is used to set up comments for your page. This is optional if you use it, but you ***really*** need to change or delete it, because it will point to the mod template's comment section out of the box.

`goatcounter_username` is mandatory to enable goatcounter. ***PLEASE*** also set this up for yourself or disable it entirely, since your page visits will be tracked by DreamWeave's goatcounter instance if you don't.

Go back to the top.

### The Fun Stuff

Set an appropriate `title` and `logo_text` for your page title and home button, respectively.

Decide whether or not you want to enable search with `build_search_index`, and RSS/Atom feeds with `generate_feeds`.
You don't have to do anything except pick between `true` or `false` here.

If you wish to use `taxonomies` to organize your pages better, now's a good time to do so. The template only includes one taxonomy, `tags`, for ease of use.

If you want to host more than just mods here, such as a blog or something, look at `menu_items` and set up the relevant pages.

### Styling

Now it's time to make it pretty (and your own). All values mentioned here are under `extra`.

`accent_color` and `background_color` are your primary customization features. If you're advanced, you can make new stylesheets for different palettes or something.
You can also customize the copyright text on the page, using `author`, or override the copyright HTML entirely with `copyright_html`.

`enable_post_view_navigation` turns navigation hints to other pages on or off. You can customize the prompt using `post_view_navigation_prompt`.

Combine values in `button_style` to determine the whether buttons are square/rounded, filled/hollow, or respond to hovering by showing an underline over the link.

`font_override` allows you to use one of the custom Morrowind/Star Wars themed fonts included with the template. You can use the font override on page titles only with `override_font_on_title_only`.

`default_credits` allows you to override the default text displayed by the new `credits` shortcode.

`use_custom_titles` themes your titles with a custom color and the mytsic-cards font.


## How Do I Set Up a Multi-Page Site?

Donwload the repo from your fork, and open `content/_index.md`. It should look like this:

```toml
+++

# For multi-page sites, simply delete or comment out this config option
redirect_to = "home"
sort_by = "title"

+++
```

Delete this line: `redirect_to = "home"`, commit that change.

Now, open `config.toml`, and set `extra.segment_versions` to `true`.

Commit that change and push both.

You now have a multi-page site.

## What About Single Page Sites?

For single page sites, work with the existing `index.md` under `content/home`. If you want, change `redirect_to = "home"` and the actual folder name `home` to be something more appropriate for your mod.

## For single and multi-page sites
To make new mod pages, create a folder underneath `content` according to the url you want your mod to be browsable from.

For example, the current template uses `home/`, so you're viewing this page at `https://dreamweave-mp.github.io/DreamWeave-Mod-Template/home/`.
If you make a new mod under `content/My Super Awesome Combat Mod`, then your mod page will be at `https://YOUR_GITHUB_USERNAME.github.io/YOUR_REPO_NAME/my-super-awesome-combat-mod/`

Every mod folder should have an `index.md` and a `changelog.md`. Changelog contents will be generated automatically, but the page must be created by you.
Packaging and distribution of your mod is automatically handled by the GitHub actions provided by the mod template.

The template is optimized for hosting and documenting blogs, but nothing stops you from doing a blog here, or whatever else you can come up with.

{% credits(default=true) %}
The above default text can be disabled, or replaced by editing default_credits in your config.toml
Credits are automatically underlined,
Per-line, by 40% of their width.
Except the last one.
{% end %}

