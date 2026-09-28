import unittest

from support import REPOSITORY  # noqa: F401  (puts tools/ on sys.path)
from dreamweave.versions import DECIMAL, Constraint, Version, VersionError


class VersionOrdering(unittest.TestCase):
    def test_numbers_compare_as_numbers_not_decimals(self):
        self.assertLess(Version.parse("0.9"), Version.parse("0.82"))
        self.assertLess(Version.parse("1.2.9"), Version.parse("1.2.10"))

    def test_missing_components_are_zero(self):
        self.assertEqual(Version.parse("1.2"), Version.parse("1.2.0"))
        self.assertEqual(hash(Version.parse("1")), hash(Version.parse("1.0.0")))

    def test_prerelease_rules_follow_semver(self):
        ordered = ["1.0.0-alpha", "1.0.0-alpha.1", "1.0.0-alpha.beta", "1.0.0-beta", "1.0.0-beta.2", "1.0.0-beta.11", "1.0.0-rc.1", "1.0.0"]
        versions = [Version.parse(text) for text in ordered]
        self.assertEqual(versions, sorted(versions))

    def test_build_metadata_does_not_change_precedence(self):
        self.assertEqual(Version.parse("1.0.0+linux"), Version.parse("1.0.0+windows"))

    def test_rejects_malformed_versions(self):
        for text in ["", "v1.0", "1.0.", "01.2", "1.2.3-01", "1..2", "1.2.3.4.5.6.7", "latest"]:
            with self.subTest(text=text), self.assertRaises(VersionError):
                Version.parse(text)

    def test_development_builds_sort_between_releases(self):
        released = Version.parse("1.2.0")
        development = released.next_development(4)
        self.assertEqual(str(development), "1.2.1-dev.4")
        self.assertLess(released, development)
        self.assertLess(development, Version.parse("1.2.1"))
        self.assertLess(development, released.next_development(5))

    def test_development_after_a_prerelease_stays_below_the_final_release(self):
        beta = Version.parse("2.0.0-beta.1")
        development = beta.next_development(3)
        self.assertLess(beta, development)
        self.assertLess(development, Version.parse("2.0.0-beta.2"))
        self.assertLess(development, Version.parse("2.0.0"))


class DecimalOrdering(unittest.TestCase):
    """St4sh numbers releases like decimals: 0.5, 0.51, 0.54, 0.6, 0.63, 0.9, 0.96, 0.961."""

    def parse(self, text):
        return Version.parse(text, DECIMAL)

    def test_later_numbers_compare_like_decimal_fractions(self):
        history = ["0.5", "0.51", "0.52", "0.54", "0.6", "0.61", "0.63", "0.9", "0.91", "0.96", "0.961", "0.963", "0.97", "1.0", "1.05", "1.1"]
        versions = [self.parse(text) for text in history]
        self.assertEqual(versions, sorted(versions))

    def test_trailing_zeros_do_not_count_but_leading_ones_do(self):
        self.assertEqual(self.parse("0.5"), self.parse("0.50"))
        self.assertLess(self.parse("0.05"), self.parse("0.5"))
        self.assertEqual(self.parse("1"), self.parse("1.0"))

    def test_the_first_number_is_an_integer(self):
        self.assertLess(self.parse("9.9"), self.parse("10.1"))

    def test_leading_zeros_are_only_meaningful_in_decimal(self):
        self.assertEqual(str(self.parse("0.05")), "0.05")
        with self.assertRaisesRegex(VersionError, 'versioning = "decimal"'):
            Version.parse("0.05")

    def test_development_builds_sort_after_the_release_and_before_any_successor(self):
        released = self.parse("0.963")
        development = released.next_development(2)
        self.assertEqual(str(development), "0.9631-dev.2")
        for successor in ("0.9631", "0.964", "0.97", "1.0"):
            with self.subTest(successor=successor):
                self.assertLess(released, development)
                self.assertLess(development, self.parse(successor))
        self.assertEqual(str(self.parse("1").next_development(1)), "1.001-dev.1")

    def test_schemes_do_not_mix(self):
        with self.assertRaises(TypeError):
            Version.parse("0.9") < self.parse("0.9")

    def test_constraints_use_the_target_scheme(self):
        constraint = Constraint.parse(">=0.9", DECIMAL)
        self.assertTrue(constraint.allows(self.parse("0.963")))
        self.assertFalse(constraint.allows(self.parse("0.85")))


class Constraints(unittest.TestCase):
    def test_comparators_all_must_hold(self):
        constraint = Constraint.parse(">=0.49, <0.51")
        self.assertTrue(constraint.allows(Version.parse("0.49")))
        self.assertTrue(constraint.allows(Version.parse("0.50.3")))
        self.assertFalse(constraint.allows(Version.parse("0.51")))
        self.assertFalse(constraint.allows(Version.parse("0.48.9")))

    def test_star_allows_anything(self):
        self.assertTrue(Constraint.parse("*").allows(Version.parse("0.0.1-dev.1")))

    def test_every_operator(self):
        version = Version.parse("1.5")
        expectations = {"=1.5": True, "!=1.5": False, ">1.4": True, ">=1.5": True, "<1.5": False, "<=1.5.0": True}
        for text, expected in expectations.items():
            with self.subTest(text=text):
                self.assertEqual(Constraint.parse(text).allows(version), expected)

    def test_rejects_ambiguous_grammar_with_a_hint(self):
        hints = {"^1.2": "not supported", "~1.2": "not supported", "1.2": "exactly that version", ">=1 || <3": "AND-only", "": "empty", "2+": "not a comparator"}
        for text, hint in hints.items():
            with self.subTest(text=text), self.assertRaisesRegex(VersionError, hint):
                Constraint.parse(text)


if __name__ == "__main__":
    unittest.main()
