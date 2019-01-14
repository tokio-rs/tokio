extern crate futures;
extern crate tokio_sync;

use futures::task::Task;
use tokio_sync::task::AtomicTask;

trait AssertSend: Send {}
trait AssertSync: Send {}

impl AssertSend for AtomicTask {}
impl AssertSync for AtomicTask {}

impl AssertSend for Task {}
impl AssertSync for Task {}
