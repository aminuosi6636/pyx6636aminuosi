import catCutout from '../../assets/cat-cutout.png';

export function CatSignature() {
  return <div className="cat-signature">
    <img src={catCutout} alt="" aria-hidden="true" />
    <span>制作人pyx</span>
  </div>;
}
