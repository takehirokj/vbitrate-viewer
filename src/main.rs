use clap::{App, Arg};
use ffmpeg::{format, frame, media::Type};
use plotters::prelude::*;

struct CliOptions {
  input_path: Option<String>,
  output_path: Option<String>,
  output_size: Resolution,
  bpp: bool, // bit per pixel
}

struct Resolution {
  w: u32,
  h: u32,
}

struct VideoInfo {
  w: u32,
  h: u32,
  bits: Vec<f64>,
}

fn get_video_info<P: AsRef<str>>(input_path: P) -> Result<VideoInfo, String> {
  ffmpeg::init().map_err(|e| e.to_string())?;
  let mut ictx = format::input(&input_path).map_err(|e| e.to_string())?;
  let input = ictx
    .streams()
    .best(Type::Video)
    .ok_or_else(|| "Failed to find video stream".to_string())?;
  let input_stream_idx = input.index();
  let mut decoder =
    input.codec().decoder().video().map_err(|e| e.to_string())?;
  decoder.set_parameters(input.parameters()).map_err(|e| e.to_string())?;

  let mut decoded_frame = frame::Video::empty();
  let mut packets = ictx.packets();
  let mut v_info = VideoInfo { w: 0, h: 0, bits: Vec::new() };
  while let Some(Ok((stream, packet))) = packets.next() {
    if stream.index() == input_stream_idx {
      let res = decoder.decode(&packet, &mut decoded_frame);
      if v_info.w == 0 && v_info.h == 0 {
        v_info.w = decoded_frame.width();
        v_info.h = decoded_frame.height();
      }

      if res.is_ok() {
        let bit = decoded_frame.packet().size as f64;
        v_info.bits.push(bit);
      }
    }
  }

  Ok(v_info)
}

fn draw_graph<P: AsRef<std::path::Path>>(
  datas: &[f64],
  y_label: &str,
  output_size: Resolution,
  output_path: P,
) -> Result<(), Box<dyn std::error::Error>> {
  let root = BitMapBackend::new(&output_path, (output_size.w, output_size.h))
    .into_drawing_area();
  root.fill(&WHITE)?;

  let max = datas.iter().fold(f64::NAN, |m, v| v.max(m)) as f64;
  let avg = datas.iter().sum::<f64>() / datas.len() as f64;

  let mut chart = ChartBuilder::on(&root)
    .set_label_area_size(LabelAreaPosition::Left, (10).percent_width())
    .set_label_area_size(LabelAreaPosition::Bottom, (10).percent_height())
    .build_cartesian_2d(0..(datas.len() - 1), 0.0..max * 1.2)?;
  chart
    .configure_mesh()
    .disable_x_mesh()
    .disable_y_mesh()
    .y_desc(y_label)
    .x_desc("Frame no")
    .label_style(("san-serif", (3).percent_height()))
    .draw()?;

  chart.draw_series(LineSeries::new(
    (0..).zip(datas.iter()).map(|(x, y)| (x, *y as f64)),
    &BLUE.mix(0.8),
  ))?;

  // Draw average bit
  chart.draw_series(LineSeries::new(
    (0..datas.len()).map(|x| (x, avg as f64)),
    &RED.mix(0.3),
  ))?;

  Ok(())
}

fn parse_cli() -> Result<CliOptions, String> {
  let cli = App::new(env!("CARGO_PKG_NAME"))
    .about(env!("CARGO_PKG_DESCRIPTION"))
    .version(env!("CARGO_PKG_VERSION"))
    .arg(
      Arg::with_name("input")
        .short("i")
        .required(true)
        .takes_value(true)
        .help("Sets a input file path"),
    )
    .arg(
      Arg::with_name("output")
        .short("o")
        .required(true)
        .takes_value(true)
        .help("Sets a output file path"),
    )
    .arg(
      Arg::with_name("output_size")
        .short("s")
        .takes_value(true)
        .use_delimiter(true)
        .require_delimiter(true)
        .value_delimiter(":")
        .default_value("1920:1080")
        .help("Sets a output size (width:height)"),
    )
    .arg(
      Arg::with_name("bit_per_pixel")
        .long("bpp")
        .help("Sets to output bit per pixel"),
    )
    .get_matches();

  let input_path = cli.value_of("input").map(|s| s.to_owned());
  let output_path = cli.value_of("output").map(|s| s.to_owned());
  let output_size = cli
    .values_of("output_size")
    .unwrap()
    .map(|s| s.parse::<u32>().unwrap())
    .collect::<Vec<u32>>();
  let output_size = Resolution { w: output_size[0], h: output_size[1] };
  let bpp = cli.is_present("bit_per_pixel");

  Ok(CliOptions { input_path, output_path, output_size, bpp })
}

fn main() -> Result<(), String> {
  let cli = parse_cli()?;
  let input_path = cli.input_path.unwrap();
  let output_path = cli.output_path.unwrap();
  let output_size = cli.output_size;
  let use_bpp = cli.bpp;

  let v_info = get_video_info(&input_path)?;
  let (data, y_label) = if use_bpp {
    let pix_num = v_info.w * v_info.h;
    let bpps = v_info
      .bits
      .iter()
      .map(|x| *x as f64 / pix_num as f64)
      .collect::<Vec<f64>>();
    (bpps, "bit per pixel")
  } else {
    (v_info.bits, "bit")
  };
  draw_graph(&data, y_label, output_size, &output_path)
    .map_err(|err| err.to_string())?;
  Ok(())
}

#[cfg(test)]
pub mod test {
  use super::*;
  use std::fs;
  use std::path::Path;

  #[test]
  fn draw_normal_graph() {
    let datas = [3000.0, 2000.0, 1500.0];
    let output_size = Resolution { w: 1280, h: 960 };
    let output_path = "./draw_graph_test.png";
    assert!(draw_graph(&datas, "bit", output_size, output_path).is_ok());
    assert!(Path::new(output_path).exists());
    assert!(fs::remove_file(output_path).is_ok());
  }

  #[test]
  fn get_video_info_normal() {
    // Input file is generated by a command below:
    //   ffmpeg -r 3 -t 1 -f lavfi -i testsrc -vf scale=320:180 \
    //   -vcodec libx264 -profile:v baseline -pix_fmt yuv420p testsrc_3_frames.mp4
    let input_path = "./test/testsrc_3_frames.mp4";
    let expected = [5068.0, 206.0, 174.0];
    let v_info = get_video_info(&input_path).unwrap();

    assert!(v_info.w == 320 && v_info.h == 180);
    assert!(v_info.bits.len() == expected.len());
    for (b, e) in v_info.bits.iter().zip(expected.iter()) {
      assert!(b == e);
    }
  }
}
