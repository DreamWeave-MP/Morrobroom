function debounce(func, wait) {
  var timeout;

  return function () {
    var context = this;
    var args = arguments;
    clearTimeout(timeout);

    timeout = setTimeout(function () {
      timeout = null;
      func.apply(context, args);
    }, wait);
  };
}

const TERM_WEIGHT = 40;
const NORMAL_WORD_WEIGHT = 2;
const FIRST_WORD_WEIGHT = 8;
const TEASER_MAX_WORDS = 30;
// Taken from mdbook
// The strategy is as follows:
// First, assign a value to each word in the document:
//  Words that correspond to search terms (stemmer aware): 40
//  Normal words: 2
//  First word in a sentence: 8
// Then use a sliding window with a constant number of words and count the
// sum of the values of the words within the window. Then use the window that got the
// maximum sum. If there are multiple maximas, then get the last one.
// Enclose the terms in <b>.
function makeTeaser(body, terms) {
  var stemmedTerms = terms.map(function (w) {
    return w.toLowerCase();
  });
  var termFound = false;
  var index = 0;
  var weighted = []; // contains elements of ["word", weight, index_in_document]

  // split in sentences, then words
  var sentences = body.toLowerCase().split(". ");

  for (var i in sentences) {
    var words = sentences[i].split(" ");
    var value = FIRST_WORD_WEIGHT;

    for (var j in words) {
      var word = words[j];

      if (word.length > 0) {
        for (var k in stemmedTerms) {
          if (word.toLowerCase().startsWith(stemmedTerms[k])) {
            value = TERM_WEIGHT;
            termFound = true;
          }
        }
        weighted.push([word, value, index]);
        value = NORMAL_WORD_WEIGHT;
      }

      index += word.length;
      index += 1;  // ' ' or '.' if last word in sentence
    }

    index += 1;  // because we split at a two-char boundary '. '
  }

  if (weighted.length === 0) {
    return body;
  }

  var windowWeights = [];
  var windowSize = Math.min(weighted.length, TEASER_MAX_WORDS);
  // We add a window with all the weights first
  var curSum = 0;
  for (var i = 0; i < windowSize; i++) {
    curSum += weighted[i][1];
  }
  windowWeights.push(curSum);

  for (var i = 0; i < weighted.length - windowSize; i++) {
    curSum -= weighted[i][1];
    curSum += weighted[i + windowSize][1];
    windowWeights.push(curSum);
  }

  // If we didn't find the term, just pick the first window
  var maxSumIndex = 0;
  if (termFound) {
    var maxFound = 0;
    // backwards
    for (var i = windowWeights.length - 1; i >= 0; i--) {
      if (windowWeights[i] > maxFound) {
        maxFound = windowWeights[i];
        maxSumIndex = i;
      }
    }
  }

  var teaser = [];
  var startIndex = weighted[maxSumIndex][2];
  for (var i = maxSumIndex; i < maxSumIndex + windowSize; i++) {
    var word = weighted[i];
    if (startIndex < word[2]) {
      // missing text from index to start of `word`
      teaser.push(body.substring(startIndex, word[2]));
      startIndex = word[2];
    }

    // add <em/> around search terms
    if (word[1] === TERM_WEIGHT) {
      teaser.push("<b>");
    }
    startIndex = word[2] + word[0].length;
    teaser.push(body.substring(word[2], startIndex));

    if (word[1] === TERM_WEIGHT) {
      teaser.push("</b>");
    }
  }
  teaser.push("…");
  return teaser.join("");
}

function formatSearchResultItem(item, terms, url) {
  var li = document.createElement("li");
  li.className = "search-result-item";
  li.innerHTML = `
    <a href="${url || item.item.url}" class="search-result-link">
      <div>
        <div class="search-result-title">${item.item.title}</div>
        <div class="search-result-excerpt">${makeTeaser(item.item.body, terms)}</div>
      </div>
    </a>
  `;

  li.addEventListener("mouseenter", function () {
    li.classList.add("search-result-border")
  });
  li.addEventListener("mouseleave", function () {
    li.classList.remove("search-result-border")
  });

  return li;
}

const MAX_ITEMS = 10;
const FUSE_OPTIONS = {
  ignoreLocation: true,
  isCaseSensitive: false,
  keys: [
    { name: 'title', weight: 2 },
    { name: 'body', weight: 3 }
  ],
  minMatchCharLength: 3,
  shouldSort: true,
  threshold: 0.1,
  useExtendedSearch: true,
};

function initSearch() {
  var $searchInput = document.getElementById("search");
  var $searchResults = document.querySelector(".search-results");
  var $searchResultsHeader = document.querySelector(".search-results__header");
  var $searchResultsItems = document.querySelector(".search-results__items");

  var fuse;
  var currentTerm = "";
  var selectedIndex = -1;
  var searchRoot = $searchInput.getAttribute("data-search-root");
  if (window.location.hostname == "localhost" || window.location.hostname == "127.0.0.1") {
    searchRoot = $searchInput.getAttribute("data-search-root-local") || searchRoot;
  }

  function localizeSearchUrl(ref) {
    var resultUrl = new URL(ref, window.location.href);
    if (window.location.hostname != "localhost" && window.location.hostname != "127.0.0.1") {
      return resultUrl.toString();
    }

    var localRoot = $searchInput.getAttribute("data-search-root-local");
    if (!localRoot) {
      return resultUrl.toString();
    }

    var localRootPath = new URL(localRoot, window.location.href).pathname;
    var localRootIndex = resultUrl.pathname.indexOf(localRootPath);
    if (localRootIndex < 0) {
      return resultUrl.toString();
    }

    resultUrl.protocol = window.location.protocol;
    resultUrl.host = window.location.host;
    resultUrl.pathname = resultUrl.pathname.slice(localRootIndex);
    return resultUrl.toString();
  }

  function isInSearchScope(ref) {
    if (!searchRoot) {
      return true;
    }

    try {
      var resultPath = new URL(localizeSearchUrl(ref), window.location.href).pathname;
      var rootPath = new URL(searchRoot, window.location.href).pathname;
      var rootPrefix = rootPath.endsWith('/') ? rootPath : rootPath + '/';
      return resultPath === rootPath || resultPath.startsWith(rootPrefix);
    } catch (error) {
      return false;
    }
  }
  
  var initIndex = async function () {
    if (fuse === undefined) {
      let indexPath = "/search_index.en.json";
      const indexData = document.querySelector('meta[name="search-index-data"]')

      if (window.location.hostname != "localhost" && window.location.hostname != "127.0.0.1") {
        let baseUrl = indexData.getAttribute('url');

        if (baseUrl.endsWith('/')) {
          baseUrl = baseUrl.substring(0, baseUrl.length - 1)
        }

        indexPath = baseUrl + indexPath;
      };

      indexPath = indexPath + `?d=${indexData.getAttribute("build_date")}`;
      const response = await fetch(indexPath);
      const documents = await response.json();

      fuse = new Fuse(documents, FUSE_OPTIONS);
    }

    return fuse;
  }

  function updateSelectedResult() {
    var items = $searchResultsItems.querySelectorAll(".search-result-item");
    items.forEach(function (item, index) {
      if (index === selectedIndex) {
        item.classList.add("search-result-border");
      } else {
        item.classList.remove("search-result-border");
      }
    });

    // Scroll selected item into view
    if (selectedIndex >= 0 && items[selectedIndex]) {
      items[selectedIndex].scrollIntoView({
        block: "nearest",
        behavior: "smooth",
      });
    }
  }

  $searchInput.addEventListener(
      "keyup",
      debounce(async function () {
        var term = $searchInput.value.trim();
        if (term === currentTerm) {
          return;
        }
        $searchResults.style.display = term === "" ? "none" : "block";
        $searchResultsItems.innerHTML = "";
        $searchResultsHeader.innerHTML = "";
        selectedIndex = -1;

        if (term === "") {
          currentTerm = "";
          return;
        }

        var results = (await initIndex()).search(term).filter(function (r) {
          return r.item.body !== "" && isInSearchScope(r.item.url);
        });

        if (results.length === 0) {
          $searchResultsHeader.innerHTML = `<span>No results found for <strong class="text-base-content">"${term}"</strong></span>`;
          return;
        }

        currentTerm = term;
        $searchResultsHeader.innerHTML = `<span >${results.length} result${results.length === 1 ? "" : "s"} for <strong class="text-base-content">"${term}"</strong></span>`;
        for (var i = 0; i < Math.min(results.length, MAX_ITEMS); i++) {
          if (!results[i].item.body) {
            continue;
          }
          $searchResultsItems.appendChild(
            formatSearchResultItem(results[i], term.split(" "), localizeSearchUrl(results[i].item.url)),
          );
        }
      }, 150),
    );

  $searchInput.addEventListener("keydown", function (e) {
    var items = $searchResultsItems.querySelectorAll(".search-result-item");

    if (e.key === "Escape") {
      $searchResults.style.display = "none";
      return;
    }

    if (items.length === 0) {
      return;
    }

    if (e.key === "ArrowDown") {
      e.preventDefault();
      selectedIndex = Math.min(selectedIndex + 1, items.length - 1);
      updateSelectedResult();
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      selectedIndex = Math.max(selectedIndex - 1, -1);
      updateSelectedResult();
    } else if (e.key === "Enter" && selectedIndex >= 0) {
      e.preventDefault();
      var link = items[selectedIndex].querySelector(".search-result-link");
      if (link) {
        window.location.href = link.getAttribute("href");
      }
    }
  });

  window.addEventListener('click', function(e) {
    if ($searchResults.style.display == "block" && !$searchResults.contains(e.target)) {
      $searchResults.style.display = "none";
    }
  });
}


if (document.readyState === "complete" ||
    (document.readyState !== "loading" && !document.documentElement.doScroll)
) {
  initSearch();
} else {
  document.addEventListener("DOMContentLoaded", initSearch);
}
