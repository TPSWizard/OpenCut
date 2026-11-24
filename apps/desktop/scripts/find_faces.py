"""Finds faces for the desktop editor's face-only retouch.

Run with the Python of a FaceFusion install (it has numpy, OpenCV and ONNX
Runtime) and FaceFusion's YOLO face model (`yoloface_8n.onnx`, GPL-3.0,
downloaded by FaceFusion, never shipped with this editor). Frames are read
with OpenCV, prepared as FaceFusion prepares them for that model (shrunk to
fit 640x640, padded at the bottom right, 0-1), and the confident boxes kept
after overlap removal. Prints one JSON object: the samples' start and rate
and, per sample, the faces as [cx, cy, w, h] shares of the frame.

    python find_faces.py --model yoloface_8n.onnx --media clip.mp4 --start 2 --length 6 --rate 5
    python find_faces.py --model yoloface_8n.onnx --media photo.jpg --still
"""

import argparse
import json
import sys

import cv2
import numpy
import onnxruntime

SIZE = 640
MIN_SCORE = 0.5
OVERLAP = 0.4


def detect(session, frame):
    height, width = frame.shape[:2]
    scale = min(1.0, SIZE / width, SIZE / height)
    small = cv2.resize(frame, (max(1, round(width * scale)), max(1, round(height * scale)))) if scale < 1 else frame
    padded = numpy.zeros((SIZE, SIZE, 3), dtype=numpy.float32)
    padded[: small.shape[0], : small.shape[1], :] = small
    tensor = numpy.expand_dims(padded.transpose(2, 0, 1), 0) / 255.0
    output = numpy.squeeze(session.run(None, {session.get_inputs()[0].name: tensor.astype(numpy.float32)})[0]).T
    boxes, scores = [], []
    ratio_x, ratio_y = width / small.shape[1], height / small.shape[0]
    for row in output:
        score = float(row[4])
        if score < MIN_SCORE:
            continue
        cx, cy, w, h = row[0] * ratio_x, row[1] * ratio_y, row[2] * ratio_x, row[3] * ratio_y
        boxes.append([float(cx - w / 2), float(cy - h / 2), float(w), float(h)])
        scores.append(score)
    keep = cv2.dnn.NMSBoxes(boxes, scores, MIN_SCORE, OVERLAP) if boxes else []
    faces = []
    for index in numpy.array(keep).flatten():
        x, y, w, h = boxes[int(index)]
        faces.append([(x + w / 2) / width, (y + h / 2) / height, w / width, h / height])
    return faces


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--model", required=True)
    parser.add_argument("--media", required=True)
    parser.add_argument("--start", type=float, default=0.0)
    parser.add_argument("--length", type=float, default=0.0)
    parser.add_argument("--rate", type=float, default=5.0)
    parser.add_argument("--still", action="store_true")
    args = parser.parse_args()
    session = onnxruntime.InferenceSession(args.model, providers=["CPUExecutionProvider"])
    if args.still:
        frame = cv2.imread(args.media, cv2.IMREAD_COLOR)
        if frame is None:
            print(json.dumps({"error": "The picture could not be read"}))
            return 1
        print(json.dumps({"start": 0.0, "rate": args.rate, "frames": [detect(session, frame)]}))
        return 0
    video = cv2.VideoCapture(args.media)
    if not video.isOpened():
        print(json.dumps({"error": "The video could not be read"}))
        return 1
    frames = []
    count = max(1, int(args.length * args.rate) + 1)
    for index in range(count):
        video.set(cv2.CAP_PROP_POS_MSEC, (args.start + index / args.rate) * 1000.0)
        ok, frame = video.read()
        if not ok:
            break
        frames.append(detect(session, frame))
    if not frames:
        print(json.dumps({"error": "No frames could be read from the video"}))
        return 1
    print(json.dumps({"start": args.start, "rate": args.rate, "frames": frames}))
    return 0


if __name__ == "__main__":
    sys.exit(main())
