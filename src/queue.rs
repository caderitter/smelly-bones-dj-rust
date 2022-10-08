struct Track {
    url: String,
    name: String
}

struct TrackQueue {
    channel_id: String,
    queue: Vec<Track>,
}

impl TrackQueueControl for TrackQueue {
    fn queue_track(&self, track: &Track) {
        self.queue.push(track)
    }

    fn pop_track(&self) {
        self.queue.remove(0)
    }

    fn remove_track(&self, index: i32) {
        self.queue.remove(index);
    }
}
