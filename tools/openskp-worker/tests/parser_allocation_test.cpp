// Pure owned construction-admission tests. No OpenSKP parser or model files.
// Original GreenLens code; Apache-2.0.
#include "parser_allocation_bounds.hpp"
#include <cstdint>
#include <iostream>
#include <map>
#include <functional>

namespace {
std::size_t checks = 0;
void check(bool result) {
  ++checks;
  if (!result) throw std::runtime_error("Parser allocation helper assertion failed");
}
void rejects(const std::function<void()>& work) {
  bool rejected = false;
  try { work(); } catch (const std::runtime_error&) { rejected = true; }
  check(rejected);
}
struct Attribute { std::string text; std::vector<Attribute> array; };
}
int main() {
  using namespace greenlens_parser;
  try {
    const auto maximum = std::numeric_limits<std::size_t>::max();
    check(checked_product(0, maximum) == 0);
    check(checked_product(maximum, 1) == maximum);
    check(checked_product(123, 8) == 984);
    rejects([&] { checked_product(maximum, 2); });
    require_elements(0, 0, 1); ++checks;
    require_elements(100000, 100000, 1); ++checks;
    require_elements(6, 12, 2); ++checks;
    rejects([&] { require_elements(1, 0, 1); });
    rejects([&] { require_elements(100000, 99999, 1); });
    rejects([&] { require_elements(7, 13, 2); });
    rejects([&] { require_elements(0, maximum, 0); });
    ConstructionBudget local;
    local.charge(request_limit);
    check(local.used() == request_limit);
    rejects([&] { local.charge(request_limit + 1); });
    check(local.used() == request_limit);
    rejects([&] { local.charge(maximum, 2); });
    check(local.used() == request_limit);
    for (int i=0; i<3; ++i) local.charge(request_limit);
    check(local.used() == construction_limit);
    local.charge(0); check(local.used() == construction_limit);
    rejects([&] { local.charge(1); });
    check(local.used() == construction_limit);
    std::vector<std::uint64_t> values;
    const auto before = construction.used();
    reserve(values, 32);
    check(values.empty() && values.capacity() >= 32);
    check(construction.used() - before == 32 * sizeof(std::uint64_t));
    rejects([&] { reserve(values, 1); });
    std::vector<std::uint64_t> excessive;
    const auto rejected_before = construction.used();
    rejects([&] { reserve(excessive, request_limit / sizeof(std::uint64_t) + 1); });
    check(excessive.capacity() == 0 && construction.used() == rejected_before);
    check(text("\xE4\xB8\xAD", 3) == "\xE4\xB8\xAD");
    check(text("a\0b", 3).size() == 3);
    check(text("", 0).empty());
    rejects([&] { text("", maximum); });
    rejects([&] { text("", request_limit); });
    std::map<std::string, Attribute> dictionary;
    dictionary.emplace("named", Attribute{"value", {Attribute{"nested", {}}}});
    const auto copy_before = construction.used();
    dictionary_copy(dictionary);
    const auto expected = sizeof(decltype(dictionary)::value_type) + 6 + 6 +
                          sizeof(Attribute) + 7;
    check(construction.used() - copy_before == expected);
    Attribute nested;
    Attribute* cursor = &nested;
    for (int depth=0; depth<64; ++depth) {
      cursor->array.emplace_back(); cursor = &cursor->array.back();
    }
    rejects([&] { attribute_copy(nested); });
    std::cout << "{\"schema_version\":\"greenlens.parser-construction-helper.v1\","
              << "\"assertions\":" << checks
              << ",\"skp_parses\":0,\"worker_executions\":0,\"containment_qualified\":false}\n";
    return 0;
  } catch (const std::exception& error) {
    std::cerr << error.what() << '\n'; return 1;
  }
}
