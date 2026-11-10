<?php

declare(strict_types=1);

namespace IdbSeq\Tests\E2e;

use IdbSeq\Exception\IdbSeqException;
use IdbSeq\Exception\ProtectedCollectionException;
use IdbSeq\IdbSeqClient;
use IdbSeq\Model\IdBatch;
use IdbSeq\Model\LastId;
use IdbSeq\Model\ResetResult;
use PHPUnit\Framework\TestCase;

/**
 * End-to-end flow against a running idbseqd (US-0031). Skipped unless
 * IDBSEQ_E2E_URL and IDBSEQ_E2E_ADMIN_PASSWORD are set. Uses the PSR-18 client
 * found by php-http/discovery.
 */
final class CriticalFlowTest extends TestCase
{
    private string $url = '';
    private IdbSeqClient $admin;
    private ?string $keyId = null;
    private string $collection = '';

    protected function setUp(): void
    {
        $this->url = (string) getenv('IDBSEQ_E2E_URL');
        $password = (string) getenv('IDBSEQ_E2E_ADMIN_PASSWORD');
        if ($this->url === '' || $password === '') {
            self::markTestSkipped('set IDBSEQ_E2E_URL and IDBSEQ_E2E_ADMIN_PASSWORD to run the e2e suite');
        }
        $this->collection = sprintf('e2e-php-%s-%s', base_convert((string) time(), 10, 36), bin2hex(random_bytes(4)));
        $this->admin = new IdbSeqClient($this->url);
        $this->admin->login('admin', $password);
    }

    protected function tearDown(): void
    {
        if ($this->collection === '' || !isset($this->admin)) {
            return;
        }
        $this->bestEffort(fn () => $this->admin->deleteCollection($this->collection));
        if ($this->keyId !== null) {
            $keyId = $this->keyId;
            $this->bestEffort(fn () => $this->admin->revokeKey($keyId));
        }
        $this->bestEffort(fn () => $this->admin->logout());
    }

    public function testCriticalFlow(): void
    {
        // Admin creates a key with read + generate + manage.
        $created = $this->admin->createKey("e2e-php-{$this->collection}", ['read', 'generate', 'manage']);
        $this->keyId = $created->id;
        self::assertNotNull($created->key);
        $key = new IdbSeqClient($this->url, $created->key);

        // Admin creates a protected collection.
        $collection = $this->admin->createCollection($this->collection, protected: true);
        self::assertTrue($collection->protected);
        self::assertNull($collection->lastId);

        // The key generates (single and batch) and reads `last`.
        self::assertSame(1, $key->generateId($this->collection));
        self::assertEquals(new IdBatch(2, 6, 5, 1), $key->generateIds($this->collection, 5));
        self::assertEquals(new LastId(6, 7), $key->lastId($this->collection));

        // The key may not reset a protected collection.
        try {
            $key->resetCollection($this->collection, allowRewind: true);
            self::fail('expected ProtectedCollectionException');
        } catch (ProtectedCollectionException $e) {
            self::assertSame(403, $e->status());
        }

        // The admin may.
        self::assertEquals(
            new ResetResult(6, null, 1),
            $this->admin->resetCollection($this->collection, allowRewind: true),
        );
        self::assertEquals(new LastId(null, 1), $key->lastId($this->collection));
    }

    private function bestEffort(callable $cleanup): void
    {
        try {
            $cleanup();
        } catch (IdbSeqException) {
            // Cleanup must not hide the test result; leftovers use unique names.
        }
    }
}
