<?php

declare(strict_types=1);

// Fails when line coverage in a Clover report is below a minimum percentage.
// Usage: php bin/check-coverage.php build/clover.xml 80

$usage = "usage: php bin/check-coverage.php <clover.xml> <min-percent>\n";
$file = $argv[1] ?? null;
$minimum = isset($argv[2]) && is_numeric($argv[2]) ? (float) $argv[2] : null;
if ($file === null || $minimum === null) {
    fwrite(STDERR, $usage);
    exit(2);
}

$xml = is_readable($file) ? simplexml_load_file($file) : false;
if ($xml === false) {
    fwrite(STDERR, "cannot read coverage report {$file}\n");
    exit(2);
}

$metrics = $xml->xpath('/coverage/project/metrics');
if (!is_array($metrics) || $metrics === []) {
    fwrite(STDERR, "no project metrics in {$file}\n");
    exit(2);
}

$statements = (int) $metrics[0]['statements'];
$covered = (int) $metrics[0]['coveredstatements'];
$percent = $statements === 0 ? 100.0 : $covered / $statements * 100;

printf("Line coverage: %.2f%% (%d/%d), minimum %.2f%%\n", $percent, $covered, $statements, $minimum);
exit($percent + 1e-9 < $minimum ? 1 : 0);
